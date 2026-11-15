package dev.bobix.phone.debug

import android.app.Activity
import android.appwidget.AppWidgetHost
import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.os.Bundle
import android.util.TypedValue
import android.widget.FrameLayout
import android.widget.LinearLayout
import dev.bobix.phone.surfaces.widgets.LargeWidgetReceiver
import dev.bobix.phone.surfaces.widgets.SmallWidgetReceiver
import dev.bobix.phone.surfaces.widgets.WideWidgetReceiver

/**
 * Debug builds only: shows the real widgets, drawn by the framework's widget host, in one activity, so they can be
 * photographed without a launcher. The first time the system asks, once per widget, to allow it (tap "Create"). `--es size
 * small|wide|large|all`. Release builds do not contain this.
 */
class WidgetPreview : Activity() {
    private lateinit var host: AppWidgetHost

    private val entries = listOf(
        Triple("small", SmallWidgetReceiver::class.java, 110 to 110),
        Triple("wide", WideWidgetReceiver::class.java, 250 to 110),
        Triple("large", LargeWidgetReceiver::class.java, 250 to 250),
    )

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        host = AppWidgetHost(this, 1)
        build()
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: android.content.Intent?) = build()

    /** Binds each widget (the system asks once per widget; the ids are kept) and shows it at its size. */
    private fun build() {
        val manager = AppWidgetManager.getInstance(this)
        val prefs = getSharedPreferences("widget_preview", MODE_PRIVATE)
        val which = intent.getStringExtra("size") ?: "all"
        val column = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(24, 96, 24, 24) }
        for ((name, receiver, size) in entries.filter { which == "all" || which == it.first }) {
            var id = prefs.getInt(name, -1)
            val provider = ComponentName(this, receiver)
            if (id == -1) {
                id = host.allocateAppWidgetId()
                prefs.edit().putInt(name, id).apply()
            }
            if (manager.getAppWidgetInfo(id) == null && !manager.bindAppWidgetIdIfAllowed(id, provider)) {
                startActivityForResult(
                    android.content.Intent(AppWidgetManager.ACTION_APPWIDGET_BIND)
                        .putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, id)
                        .putExtra(AppWidgetManager.EXTRA_APPWIDGET_PROVIDER, provider),
                    id,
                )
                return
            }
            val view = host.createView(this, id, manager.getAppWidgetInfo(id))
            manager.updateAppWidgetOptions(id, Bundle().apply {
                putInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH, size.first)
                putInt(AppWidgetManager.OPTION_APPWIDGET_MAX_WIDTH, size.first)
                putInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT, size.second)
                putInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT, size.second)
            })
            val frame = FrameLayout(this)
            frame.addView(view, FrameLayout.LayoutParams(dp(size.first), dp(size.second)))
            column.addView(frame, LinearLayout.LayoutParams(dp(size.first), dp(size.second)).apply { bottomMargin = dp(12) })
        }
        setContentView(android.widget.ScrollView(this).apply { addView(column) })
        host.startListening()
    }

    private fun dp(v: Int) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v.toFloat(), resources.displayMetrics).toInt()

    override fun onDestroy() {
        host.stopListening()
        super.onDestroy()
    }
}
