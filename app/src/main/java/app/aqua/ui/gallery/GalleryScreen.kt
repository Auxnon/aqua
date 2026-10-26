package app.aqua.ui.gallery

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.aqua.AquaApp
import app.aqua.data.Artwork
import app.aqua.engine.Native
import app.aqua.ui.theme.AquaColors
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.text.DateFormat
import java.util.Date
import kotlin.math.max
import kotlin.random.Random

@Composable
fun GalleryScreen(onOpen: (File) -> Unit) {
    val app = LocalContext.current.applicationContext as AquaApp
    val scope = rememberCoroutineScope()
    var items by remember { mutableStateOf<List<Artwork>>(emptyList()) }
    var creating by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }

    fun reload() {
        scope.launch(Dispatchers.IO) {
            app.repo.prune()
            val l = app.repo.list()
            withContext(Dispatchers.Main) { items = l }
        }
    }
    LaunchedEffect(Unit) { reload() }

    fun open(dir: File) {
        if (busy) return
        busy = true
        scope.launch(Dispatchers.IO) {
            val ok = Native.load(app.engine, dir.path)
            withContext(Dispatchers.Main) {
                busy = false
                if (ok) onOpen(dir)
            }
        }
    }

    Box(Modifier.fillMaxSize().background(AquaColors.Background)) {
        Column(Modifier.fillMaxSize().statusBarsPadding()) {
            Text(
                "Aqua",
                color = AquaColors.Text,
                fontSize = 28.sp,
                fontWeight = FontWeight.Light,
                modifier = Modifier.padding(start = 20.dp, top = 18.dp, bottom = 12.dp),
            )
            if (items.isEmpty()) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Text("Tap + to start a painting", color = AquaColors.TextDim)
                }
            } else {
                LazyVerticalGrid(
                    columns = GridCells.Adaptive(170.dp),
                    contentPadding = PaddingValues(14.dp, 4.dp, 14.dp, 100.dp),
                    horizontalArrangement = Arrangement.spacedBy(14.dp),
                    verticalArrangement = Arrangement.spacedBy(18.dp),
                ) {
                    items(items, key = { it.id }) { art ->
                        ArtCard(
                            art,
                            onClick = { open(art.dir) },
                            onDelete = { app.repo.delete(art); reload() },
                            onDuplicate = { scope.launch(Dispatchers.IO) { app.repo.duplicate(art); reload() } },
                        )
                    }
                }
            }
        }
        FloatingActionButton(
            onClick = { creating = true },
            containerColor = AquaColors.Accent,
            modifier = Modifier.align(Alignment.BottomEnd).navigationBarsPadding().padding(22.dp),
        ) { Icon(Icons.Outlined.Add, "New painting") }
    }

    if (creating) {
        NewCanvasDialog(
            onDismiss = { creating = false },
            onCreate = { w, h, paper ->
                creating = false
                busy = true
                scope.launch(Dispatchers.IO) {
                    val dir = app.repo.newDir()
                    Native.newCanvas(app.engine, w, h, paper, Random.nextInt(1, Int.MAX_VALUE))
                    Native.save(app.engine, dir.path)
                    withContext(Dispatchers.Main) {
                        busy = false
                        onOpen(dir)
                    }
                }
            },
        )
    }
}

@Composable
private fun ArtCard(art: Artwork, onClick: () -> Unit, onDelete: () -> Unit, onDuplicate: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    var confirm by remember { mutableStateOf(false) }
    val thumb by produceState<ImageBitmap?>(null, art.thumb, art.modified) {
        value = withContext(Dispatchers.IO) { BitmapFactory.decodeFile(art.thumb.path)?.asImageBitmap() }
    }
    Column {
        Box(
            Modifier
                .fillMaxWidth()
                .aspectRatio(art.width / max(1f, art.height.toFloat()))
                .clip(RoundedCornerShape(6.dp))
                .border(1.dp, AquaColors.Line, RoundedCornerShape(6.dp))
                .background(AquaColors.Raised)
                .combinedClickable(onClick = onClick, onLongClick = { menu = true }),
        ) {
            thumb?.let { Image(it, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize()) }
            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                DropdownMenuItem(text = { Text("Duplicate") }, onClick = { menu = false; onDuplicate() })
                DropdownMenuItem(text = { Text("Delete", color = AquaColors.Danger) }, onClick = { menu = false; confirm = true })
            }
        }
        Text(
            DateFormat.getDateInstance(DateFormat.MEDIUM).format(Date(art.modified)),
            color = AquaColors.TextDim,
            fontSize = 12.sp,
            modifier = Modifier.padding(top = 6.dp, start = 2.dp),
        )
    }
    if (confirm) {
        AlertDialog(
            onDismissRequest = { confirm = false },
            title = { Text("Delete painting?") },
            text = { Text("This can't be undone.") },
            confirmButton = { TextButton(onClick = { confirm = false; onDelete() }) { Text("Delete", color = AquaColors.Danger) } },
            dismissButton = { TextButton(onClick = { confirm = false }) { Text("Cancel") } },
        )
    }
}

private data class SizePreset(val label: String, val w: Int, val h: Int)

@Composable
private fun NewCanvasDialog(onDismiss: () -> Unit, onCreate: (Int, Int, Int) -> Unit) {
    val window = LocalWindowInfo.current.containerSize
    val screen = remember(window) {
        // Screen-shaped canvas, longest side capped to keep the wet simulation light.
        val s = minOf(1f, 2400f / max(window.width, window.height).coerceAtLeast(1))
        SizePreset("Screen", (window.width * s).toInt().coerceAtLeast(512), (window.height * s).toInt().coerceAtLeast(512))
    }
    val presets = listOf(
        screen,
        SizePreset("Square", 2048, 2048),
        SizePreset("Portrait", 1536, 2048),
        SizePreset("Landscape", 2048, 1536),
        SizePreset("Postcard", 1800, 1200),
    )
    val papers = listOf("Cold press", "Hot press", "Rough")
    var size by remember { mutableStateOf(presets[0]) }
    var paper by remember { mutableStateOf(0) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("New painting") },
        text = {
            Column {
                Text("Size", color = AquaColors.TextDim, fontSize = 12.sp)
                Spacer(Modifier.height(6.dp))
                presets.forEach { p ->
                    Choice("${p.label}  ·  ${p.w}×${p.h}", p == size) { size = p }
                }
                Spacer(Modifier.height(12.dp))
                Text("Paper", color = AquaColors.TextDim, fontSize = 12.sp)
                Spacer(Modifier.height(6.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    papers.forEachIndexed { i, name -> Choice(name, i == paper) { paper = i } }
                }
            }
        },
        confirmButton = { TextButton(onClick = { onCreate(size.w, size.h, paper) }) { Text("Create") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

@Composable
private fun Choice(label: String, selected: Boolean, onClick: () -> Unit) {
    Text(
        label,
        color = if (selected) AquaColors.Background else AquaColors.Text,
        fontSize = 13.sp,
        modifier = Modifier
            .padding(vertical = 2.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(if (selected) AquaColors.Accent else AquaColors.Raised)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 7.dp),
    )
}

