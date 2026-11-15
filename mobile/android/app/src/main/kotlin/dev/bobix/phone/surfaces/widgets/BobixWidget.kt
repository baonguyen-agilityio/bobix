package dev.bobix.phone.surfaces.widgets

import android.content.Context
import android.content.Intent
import android.graphics.BitmapFactory
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.provideContent
import androidx.glance.background
import androidx.glance.color.ColorProvider
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider
import dev.bobix.phone.R
import dev.bobix.phone.surfaces.SurfaceList
import dev.bobix.phone.surfaces.SurfaceState
import dev.bobix.phone.surfaces.SurfaceTick
import dev.bobix.phone.surfaces.SurfaceView
import dev.bobix.phone.surfaces.Surfaces
import dev.bobix.phone.surfaces.needText
import dev.bobix.phone.surfaces.stateText
import dev.bobix.phone.surfaces.tileText

enum class WidgetSize(val rows: Int, val others: Int) { SMALL(0, 0), WIDE(2, 0), LARGE(4, 2) }

/** The prototype's tokens as the widget needs them, light and dark. */
object Palette {
    val bg = ColorProvider(day = Color(0xFFF4F5F9), night = Color(0xFF14161B))
    val surface = ColorProvider(day = Color(0xFFFFFFFF), night = Color(0xFF1D2027))
    val ink = ColorProvider(day = Color(0xFF1A1C22), night = Color(0xFFECEEF3))
    val ink3 = ColorProvider(day = Color(0xFF5B5F6B), night = Color(0xFF9AA0AE))
    val accent = ColorProvider(day = Color(0xFFC23F26), night = Color(0xFFE4573C))
    val dim = ColorProvider(day = Color(0x995B5F6B), night = Color(0x999AA0AE))
}

/**
 * One widget class per size (Glance maps a widget id to its class, so a shared class would draw the small layout on the large
 * widget). The list is read inside the composition, keyed on [SurfaceTick]: a widget whose session is running redraws when
 * [Surfaces.refresh] bumps the tick, and one without a session reads afresh in `provideGlance`. Every tap opens the app
 * (the first waiting request when something waits, Home otherwise). Nothing answers.
 */
abstract class BobixWidget(private val size: WidgetSize) : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        provideContent {
            val tick by SurfaceTick.version.collectAsState()
            val drawn = remember(tick) { Drawn.read(context, size) }
            WidgetContent(context, drawn.view, size, actionStartActivity(drawn.open), drawn.pets, drawn.first)
        }
    }
}

class SmallWidget : BobixWidget(WidgetSize.SMALL)
class WideWidget : BobixWidget(WidgetSize.WIDE)
class LargeWidget : BobixWidget(WidgetSize.LARGE)

/** What one draw needs, read together so a redraw never mixes two versions of the list. */
private class Drawn(val view: SurfaceView, val open: Intent, val pets: Map<String, ImageProvider?>, val first: ImageProvider?) {
    companion object {
        fun read(context: Context, size: WidgetSize): Drawn {
            val view = Surfaces.view(context)
            val pets = view.rows.take(size.rows).mapNotNull { it.pet }.associate { it.petFile to bitmap(context, it) }
            val first = view.rows.firstOrNull()?.pet?.let { bitmap(context, it) }
            return Drawn(view, Surfaces.openFirst(context, view), pets, first)
        }

        private fun bitmap(context: Context, pet: SurfaceList.Pet): ImageProvider? =
            Surfaces.store(context).petFile(pet.petFile)?.let { f -> BitmapFactory.decodeFile(f.path)?.let { ImageProvider(it) } }
    }
}

@androidx.compose.runtime.Composable
fun WidgetContent(
    context: Context,
    v: SurfaceView,
    size: WidgetSize,
    open: androidx.glance.action.Action,
    pets: Map<String, ImageProvider?>,
    firstPet: ImageProvider?,
) {
    val stale = v.state == SurfaceState.STALE
    val ink = if (stale) Palette.dim else Palette.ink
    val sub = if (stale) Palette.dim else Palette.ink3
    Box(
        modifier = GlanceModifier.fillMaxSize().background(Palette.bg).cornerRadius(22.dp).padding(if (size == WidgetSize.SMALL) 8.dp else 12.dp).clickable(open),
    ) {
        when (size) {
            WidgetSize.SMALL -> Column(modifier = GlanceModifier.fillMaxSize(), verticalAlignment = Alignment.CenterVertically, horizontalAlignment = Alignment.CenterHorizontally) {
                Head(context, v, ink, sub, firstPet, big = true)
            }
            WidgetSize.WIDE -> Row(modifier = GlanceModifier.fillMaxSize(), verticalAlignment = Alignment.CenterVertically) {
                Column(modifier = GlanceModifier.defaultWeight()) { Head(context, v, ink, sub, firstPet, big = false) }
                if (v.rows.isNotEmpty()) {
                    Spacer(GlanceModifier.width(8.dp))
                    Column(modifier = GlanceModifier.defaultWeight()) { Rows(context, v, size, pets, ink, sub) }
                }
            }
            WidgetSize.LARGE -> Column(modifier = GlanceModifier.fillMaxSize()) {
                Head(context, v, ink, sub, firstPet, big = false)
                Spacer(GlanceModifier.height(8.dp))
                Rows(context, v, size, pets, ink, sub)
                if (v.others.isNotEmpty()) {
                    Spacer(GlanceModifier.height(6.dp))
                    for (o in v.others.take(size.others)) {
                        Text(o.name, style = TextStyle(color = sub, fontSize = 12.sp), maxLines = 1)
                    }
                }
                if (v.computer.isNotEmpty()) {
                    Spacer(GlanceModifier.defaultWeight())
                    Text(v.computer, style = TextStyle(color = sub, fontSize = 11.sp), maxLines = 1)
                }
            }
        }
    }
}

@androidx.compose.runtime.Composable
private fun Head(context: Context, v: SurfaceView, ink: ColorProvider, sub: ColorProvider, pet: ImageProvider?, big: Boolean) {
    val accent = if (v.state == SurfaceState.STALE) Palette.dim else Palette.accent
    when (v.state) {
        SurfaceState.NEEDS_YOU -> {
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (pet != null && !v.hidden) {
                    Image(pet, contentDescription = null, modifier = GlanceModifier.size(if (big) 34.dp else 30.dp))
                    Spacer(GlanceModifier.width(6.dp))
                }
                Text(needText(context, v.count), style = TextStyle(color = accent, fontSize = if (big) 15.sp else 17.sp, fontWeight = FontWeight.Bold), maxLines = 2)
            }
            if (v.hidden) Text(context.getString(R.string.phone_wg_hidden), style = TextStyle(color = sub, fontSize = 11.sp), maxLines = 1)
        }
        SurfaceState.STALE -> {
            Text(context.getString(R.string.phone_wg_updated, v.updatedAt), style = TextStyle(color = ink, fontSize = if (big) 15.sp else 17.sp, fontWeight = FontWeight.Medium), maxLines = 2)
            stateText(context, v)?.let { Text(it, style = TextStyle(color = sub, fontSize = 11.sp), maxLines = 2) }
        }
        else -> Text(tileText(context, v), style = TextStyle(color = ink, fontSize = if (big) 15.sp else 17.sp, fontWeight = FontWeight.Medium), maxLines = 3)
    }
}

@androidx.compose.runtime.Composable
private fun Rows(context: Context, v: SurfaceView, size: WidgetSize, pets: Map<String, ImageProvider?>, ink: ColorProvider, sub: ColorProvider) {
    for (r in v.rows.take(size.rows)) {
        Row(modifier = GlanceModifier.fillMaxWidth().padding(vertical = 2.dp), verticalAlignment = Alignment.CenterVertically) {
            val img = r.pet?.let { pets[it.petFile] }
            if (img != null) {
                Image(img, contentDescription = null, modifier = GlanceModifier.size(24.dp))
                Spacer(GlanceModifier.width(6.dp))
            }
            Text(r.name ?: context.getString(R.string.phone_push_generic), style = TextStyle(color = ink, fontSize = 13.sp), maxLines = 1, modifier = GlanceModifier.defaultWeight())
        }
    }
}

/** Every widget update also sets the stale alarm again: it is lost at a reboot, and a widget on the screen is the reason to have it. */
abstract class SurfaceWidgetReceiver : GlanceAppWidgetReceiver() {
    override fun onUpdate(context: Context, appWidgetManager: android.appwidget.AppWidgetManager, appWidgetIds: IntArray) {
        super.onUpdate(context, appWidgetManager, appWidgetIds)
        Surfaces.scheduleStale(context)
    }
}

class SmallWidgetReceiver : SurfaceWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = SmallWidget()
}

class WideWidgetReceiver : SurfaceWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = WideWidget()
}

class LargeWidgetReceiver : SurfaceWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = LargeWidget()
}
