package dev.smix.fixture

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp

/// A list and a detail in one activity, the detail drawn over a list that
/// stays composed underneath.
///
/// This is how a Compose app navigates without a second activity, and it
/// is a shape a bounded reading of the screen cannot tell apart: the
/// window, the activity and every node near the root are the same with
/// the detail open and closed. A back that closed the detail was
/// answered `gaveUp` on a screen that had gone back.
///
/// The nesting is deliberate — a scaffold, a content area, then the
/// screens — because that is where real screens put the difference.
class NavActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            var open by remember { mutableStateOf<Int?>(null) }
            BackHandler(enabled = open != null) { open = null }
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .semantics { testTagsAsResourceId = true }
                    .testTag("nav_scaffold"),
            ) {
                Text("nav fixture", modifier = Modifier.testTag("nav_title").padding(16.dp))
                Box(modifier = Modifier.fillMaxSize().testTag("nav_content")) {
                    Column(modifier = Modifier.testTag("nav_list")) {
                        for (i in 1..8) {
                            Box(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .testTag("nav_card_$i")
                                    .clickable { open = i }
                                    .padding(16.dp),
                            ) {
                                Column {
                                    Text("card $i")
                                    Text("subtitle $i")
                                }
                            }
                        }
                    }
                    open?.let { i ->
                        Column(
                            modifier = Modifier
                                .fillMaxSize()
                                .background(Color.White)
                                .testTag("nav_detail")
                                .padding(16.dp),
                        ) {
                            Text("detail $i", modifier = Modifier.testTag("nav_detail_title"))
                            Text("more about $i")
                        }
                    }
                }
            }
        }
    }
}
