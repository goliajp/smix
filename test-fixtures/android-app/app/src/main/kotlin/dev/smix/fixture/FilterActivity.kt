package dev.smix.fixture

import android.app.Activity
import android.os.Bundle
import android.text.Editable
import android.text.TextWatcher
import android.view.ViewGroup
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView

/// A list narrowed by what is typed above it, redrawn the way a consumer's
/// device search redraws it: the whole data set replaced and
/// `notifyDataSetChanged`. The rows that drop out must leave the tree with
/// the screen; a consumer saw all eight still reported after one remained.
class FilterActivity : Activity() {
    private val all = listOf(
        "Lobby Entrance", "Lobby Reception", "Lobby Elevator", "Parking North",
        "Parking South", "Loading Dock", "Server Room", "Roof Access",
    )
    private var shown = all

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val count = TextView(this).apply { text = "${all.size} results found" }
        val adapter = Rows()
        val list = RecyclerView(this).apply {
            id = R.id.fixture_filter_list
            layoutManager = LinearLayoutManager(this@FilterActivity)
            this.adapter = adapter
        }
        val input = EditText(this).apply {
            id = R.id.fixture_filter_input
            hint = "search"
            addTextChangedListener(object : TextWatcher {
                override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) {}
                override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) {}
                override fun afterTextChanged(s: Editable?) {
                    val q = s?.toString().orEmpty()
                    shown = all.filter { it.contains(q, ignoreCase = true) }
                    count.text = if (shown.size == 1) "1 result found" else "${shown.size} results found"
                    adapter.notifyDataSetChanged()
                }
            })
        }
        setContentView(
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                // clear of the status bar, which an edge-to-edge window draws over
                setPadding(0, 200, 0, 0)
                addView(input)
                addView(count)
                addView(list, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
            },
        )
    }

    private inner class Rows : RecyclerView.Adapter<RecyclerView.ViewHolder>() {
        override fun getItemCount() = shown.size
        override fun onCreateViewHolder(parent: ViewGroup, type: Int) =
            object : RecyclerView.ViewHolder(
                TextView(parent.context).apply {
                    id = R.id.fixture_filter_row
                    textSize = 20f
                    setPadding(32, 40, 32, 40)
                    isClickable = true
                    layoutParams = RecyclerView.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.WRAP_CONTENT,
                    )
                },
            ) {}

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            (holder.itemView as TextView).text = shown[position]
        }
    }
}
