package dev.smix.runner

import dev.smix.runner.WindowResize.Box
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test

// The two dumps are cut from an android-desktop emulator (API 34,
// 3840x2160 at 320 dpi): the fixture's window in front, the launcher behind.
class WindowResizeTest {
    private val stack = """
        RootTask id=23 bounds=[1000,1000][2678,2160] displayId=0 userId=0
         configuration={1.0 310mcc260mnc [en_US] ldltr sw580dp w839dp h580dp 320dpi lrg land winConfig={ mBounds=Rect(1000, 1000 - 2678, 2160) mWindowingMode=freeform mDisplayWindowingMode=freeform mActivityType=standard mAlwaysOnTop=undefined mRotation=ROTATION_0} s.19}
          taskId=23: dev.smix.fixture/dev.smix.fixture.DesktopActivity bounds=[1000,1000][2678,2160] userId=0 visible=true topActivity=ComponentInfo{dev.smix.fixture/dev.smix.fixture.DesktopActivity}

        RootTask id=1 bounds=[0,0][3840,2160] displayId=0 userId=0
         configuration={1.0 310mcc260mnc [en_US] ldltr sw1080dp w1920dp h1032dp 320dpi xlrg long land winConfig={ mBounds=Rect(0, 0 - 3840, 2160) mWindowingMode=fullscreen mDisplayWindowingMode=freeform mActivityType=home mAlwaysOnTop=undefined mRotation=ROTATION_0} s.19}
          taskId=2: org.chromium.home.pc/org.chromium.home.pc.HostActivity bounds=[0,0][3840,2160] userId=0 visible=true topActivity=ComponentInfo{org.chromium.home.pc/org.chromium.home.pc.HostActivity}
    """.trimIndent()

    private val displays = """
        WINDOW MANAGER DISPLAY CONTENTS (dumpsys window displays)
          Display: mDisplayId=0 (organized)
            init=3840x2160 320dpi mMinSizeOfResizeableTaskDp=220 cur=3840x2160 app=3840x2064 rng=2160x2064-3840x3744
            mDecorInsetsInfo:
              ROTATION_0={nonDecorInsets=[0,0][0,96], configInsets=[0,0][0,96], nonDecorFrame=[0,0][3840,2064], configFrame=[0,0][3840,2064]}
              ROTATION_90={nonDecorInsets=[0,0][0,96], configInsets=[0,0][0,96], nonDecorFrame=[0,0][2160,3744], configFrame=[0,0][2160,3744]}
          DisplayRotation
            mCurrentAppOrientation=SCREEN_ORIENTATION_UNSPECIFIED
            mRotation=0 mDeferredRotationPauseCount=0
    """.trimIndent()

    @Test
    fun theAppsTaskIsFoundByPackageOrAsTheAppInFront() {
        val byName = WindowResize.findTask(stack, "dev.smix.fixture")!!
        assertEquals(23, byName.id)
        assertEquals(Box(1000, 1000, 2678, 2160), byName.bounds)
        assertEquals(true, byName.freeform)
        assertEquals(byName, WindowResize.findTask(stack, null))
    }

    @Test
    fun theLauncherIsNotTheAppInFrontAndItsModeIsItsOwn() {
        val home = WindowResize.findTask(stack, "org.chromium.home.pc")!!
        assertEquals(false, home.freeform)
        val launcherOnly = stack.lines().drop(4).joinToString("\n")
        assertNull(WindowResize.findTask(launcherOnly, null))
        assertNull(WindowResize.findTask(stack, "dev.smix.absent"))
    }

    @Test
    fun theAppAreaIsTheFrameOfTheCurrentRotation() {
        val d = WindowResize.readDisplay(displays)!!
        assertEquals(Box(0, 0, 3840, 2064), d.appArea)
        assertEquals(220, d.minSideDp)
        val turned = WindowResize.readDisplay(displays.replace("mRotation=0 ", "mRotation=1 "))!!
        assertEquals(Box(0, 0, 2160, 3744), turned.appArea)
        assertNull(WindowResize.readDisplay(displays.replace("mMinSizeOfResizeableTaskDp=220", "")))
    }

    @Test
    fun aWindowStaysWhereItIsUnlessItWouldLeaveTheAppArea() {
        val area = Box(0, 0, 3840, 2064)
        assertEquals(Box(100, 200, 1100, 900), WindowResize.place(Box(100, 200, 500, 500), 1000, 700, area))
        // the window that hung below the taskbar is lifted onto the area
        assertEquals(Box(1000, 528, 3048, 2064), WindowResize.place(Box(1000, 1000, 2678, 2160), 2048, 1536, area))
        assertEquals(Box(0, 0, 3840, 2064), WindowResize.place(Box(-50, -50, 10, 10), 3840, 2064, area))
        assertNull(WindowResize.place(Box(0, 0, 10, 10), 3841, 100, area))
    }

    @Test
    fun dpAndPixelsConvertAtTheDisplaysDensity() {
        assertEquals(2048, WindowResize.px(1024, 2f))
        assertEquals(839, WindowResize.dp(1678, 2f))
        assertEquals(
            "am task resize 23 1 2 3 4",
            WindowResize.resizeCommand(WindowResize.findTask(stack, null)!!, Box(1, 2, 3, 4)),
        )
    }

    @Test
    fun aRequestNamesBothSides() {
        assertEquals(
            WindowResize.Request("a.b", 1024, 768),
            WindowResize.Request.decode("""{"package":"a.b","widthDp":1024,"heightDp":768}"""),
        )
        assertEquals(null, WindowResize.Request.decode("""{"widthDp":1,"heightDp":2}""").pkg)
        val e = assertThrows(IllegalArgumentException::class.java) {
            WindowResize.Request.decode("""{"widthDp":1024}""")
        }
        assertEquals("heightDp is required", e.message)
        assertThrows(IllegalArgumentException::class.java) {
            WindowResize.Request.decode("""{"widthDp":0,"heightDp":2}""")
        }
    }

    @Test
    fun theDisplaySizeIsWhatItMeasuresTurnedAsItIsNow() {
        assertEquals(Box(0, 0, 3840, 2160), WindowResize.readDisplaySize(displays))
        assertNull(WindowResize.readDisplaySize(displays.replace("cur=3840x2160", "")))
        assertNull(WindowResize.readDisplaySize("no display here"))
    }

    @Test
    fun aWindowIsNamedByPackageOrIsTheAppInFront() {
        assertEquals(WindowResize.Which("a.b"), WindowResize.Which.decode("""{"package":"a.b"}"""))
        assertEquals(WindowResize.Which(null), WindowResize.Which.decode("{}"))
        assertEquals(WindowResize.Which(null), WindowResize.Which.decode(""))
        assertThrows(IllegalArgumentException::class.java) { WindowResize.Which.decode("nope") }
    }
}
