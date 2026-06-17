package com.duxweb.codux

import android.os.Build
import android.os.Bundle
import android.view.ViewGroup
import io.flutter.embedding.android.FlutterActivity

class MainActivity : FlutterActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        disableDefaultFocusHighlight(window.decorView)
    }

    private fun disableDefaultFocusHighlight(view: android.view.View) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            view.defaultFocusHighlightEnabled = false
        }
        if (view is ViewGroup) {
            for (index in 0 until view.childCount) {
                disableDefaultFocusHighlight(view.getChildAt(index))
            }
        }
    }
}
