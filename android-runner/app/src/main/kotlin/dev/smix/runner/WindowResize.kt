package dev.smix.runner

import org.json.JSONObject

/// An app's desktop window set to a size in dp, read from and sent to the
/// window manager through `am` and `dumpsys`, which is what the shell can
/// reach of it.
///
/// `am task resize` takes any bounds it is given: a window larger than the
/// display, or below the taskbar, is placed there as asked, and a task id
/// that names nothing answers as if it had worked. So the window is placed
/// inside the display's app area here, and its size is read back.
object WindowResize {
    data class Box(val left: Int, val top: Int, val right: Int, val bottom: Int) {
        val width: Int get() = right - left
        val height: Int get() = bottom - top
    }

    data class Task(val id: Int, val pkg: String, val bounds: Box, val freeform: Boolean)

    /// What the display allows: the area apps are laid out in (the display
    /// less its taskbar) and the smallest window side, in dp.
    data class Display(val appArea: Box, val minSideDp: Int)

    /// `pkg` null means the app in front.
    data class Which(val pkg: String?) {
        companion object {
            fun decode(body: String): Which {
                if (body.isBlank()) return Which(null)
                val o = try {
                    JSONObject(body)
                } catch (e: org.json.JSONException) {
                    throw IllegalArgumentException("body is not JSON: ${e.message}")
                }
                return Which(if (o.isNull("package")) null else o.getString("package"))
            }
        }
    }

    /// `pkg` null means the app in front.
    data class Request(val pkg: String?, val widthDp: Int, val heightDp: Int) {
        companion object {
            fun decode(body: String): Request {
                val o = try {
                    JSONObject(body)
                } catch (e: org.json.JSONException) {
                    throw IllegalArgumentException("body is not JSON: ${e.message}")
                }
                fun side(name: String): Int {
                    require(o.has(name)) { "$name is required" }
                    val v = o.getInt(name)
                    require(v > 0) { "$name must be a positive number of dp, not $v" }
                    return v
                }
                val pkg = if (o.isNull("package")) null else o.getString("package")
                return Request(pkg, side("widthDp"), side("heightDp"))
            }
        }
    }

    private val ROOT = Regex("""^RootTask id=\d+""")
    private val MODE = Regex("""mWindowingMode=(\w+)""")
    private val TYPE = Regex("""mActivityType=(\w+)""")
    private val TASK = Regex(
        """^\s*taskId=(\d+): ([^/\s]+)/\S+ bounds=\[(-?\d+),(-?\d+)]\[(-?\d+),(-?\d+)].*\bvisible=true""",
    )

    /// The visible task of `pkg` from `am stack list`, or with `pkg` null
    /// the first visible app task (not the launcher). Root tasks are listed
    /// front first, each with a configuration line naming its windowing
    /// mode and activity type, then its tasks.
    fun findTask(stackList: String, pkg: String?): Task? {
        var mode = ""
        var type = ""
        for (line in stackList.lines()) {
            if (ROOT.containsMatchIn(line)) {
                mode = ""
                type = ""
                continue
            }
            if (mode.isEmpty()) MODE.find(line)?.let { mode = it.groupValues[1] }
            if (type.isEmpty()) TYPE.find(line)?.let { type = it.groupValues[1] }
            val m = TASK.find(line) ?: continue
            val (id, p, l, t, r, b) = m.destructured
            val wanted = if (pkg != null) p == pkg else type == "standard"
            if (wanted) return Task(id.toInt(), p, Box(l.toInt(), t.toInt(), r.toInt(), b.toInt()), mode == "freeform")
        }
        return null
    }

    private val MIN_SIDE = Regex("""mMinSizeOfResizeableTaskDp=(\d+)""")
    private val ROTATION = Regex("""^\s*mRotation=(\d)""")
    // Android 14 names the app area `configFrame`; 16 `overrideConfigFrame`
    private val FRAME = Regex("""ROTATION_(\d+)=\{.*\b(?:configFrame|overrideConfigFrame)=\[(\d+),(\d+)]\[(\d+),(\d+)]""")

    /// The first display's app area and smallest window side, from
    /// `dumpsys window displays`; null when either is missing from it.
    fun readDisplay(dump: String): Display? {
        val first = dump.split(Regex("""(?m)^\s*Display: mDisplayId=""")).getOrNull(1) ?: return null
        val minSide = MIN_SIDE.find(first)?.groupValues?.get(1)?.toInt() ?: return null
        // mRotation counts quarter turns; the frames are named in degrees
        val quarter = first.lines().firstNotNullOfOrNull { ROTATION.find(it)?.groupValues?.get(1) } ?: return null
        val degrees = (quarter.toInt() * 90).toString()
        val frame = first.lines()
            .mapNotNull { FRAME.find(it) }
            .firstOrNull { it.groupValues[1] == degrees }
            ?: return null
        val (_, l, t, r, b) = frame.destructured
        return Display(Box(l.toInt(), t.toInt(), r.toInt(), b.toInt()), minSide)
    }

    private val CUR = Regex("""\bcur=(\d+)x(\d+)""")

    /// The first display's size in px as it is turned now, from
    /// `dumpsys window displays`: what a screenshot of it measures.
    fun readDisplaySize(dump: String): Box? {
        val first = dump.split(Regex("""(?m)^\s*Display: mDisplayId=""")).getOrNull(1) ?: return null
        val (w, h) = CUR.find(first)?.destructured ?: return null
        return Box(0, 0, w.toInt(), h.toInt())
    }

    /// Where a window `w` × `h` px goes: where it is, moved only as far as
    /// it takes to lie inside `area`. Null when it cannot fit.
    fun place(current: Box, w: Int, h: Int, area: Box): Box? {
        if (w > area.width || h > area.height) return null
        val left = current.left.coerceIn(area.left, area.right - w)
        val top = current.top.coerceIn(area.top, area.bottom - h)
        return Box(left, top, left + w, top + h)
    }

    fun px(dp: Int, density: Float): Int = Math.round(dp * density)

    fun dp(px: Int, density: Float): Int = Math.round(px / density)

    fun resizeCommand(task: Task, to: Box): String =
        "am task resize ${task.id} ${to.left} ${to.top} ${to.right} ${to.bottom}"
}
