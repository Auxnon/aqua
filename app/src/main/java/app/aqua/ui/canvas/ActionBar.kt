package app.aqua.ui.canvas

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.MoreVert
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.aqua.data.PushState
import app.aqua.data.Settings
import app.aqua.data.Sync
import app.aqua.ui.theme.AquaColors
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.text.DateFormat
import java.util.Date

/** A top-bar action: a button while there is room for it, otherwise an item in the ⋮ menu. */
class BarAction(
    val label: String,
    val selected: Boolean = false,
    val menuIcon: @Composable () -> Unit,
    val button: @Composable (size: Dp) -> Unit,
    val onClick: () -> Unit,
)

fun iconAction(icon: ImageVector, label: String, selected: Boolean = false, tint: Color? = null, onClick: () -> Unit) = BarAction(
    label,
    selected,
    menuIcon = { Icon(icon, null, tint = tint ?: if (selected) AquaColors.Accent else AquaColors.Text, modifier = Modifier.size(22.dp)) },
    button = { s -> RoundButton(icon, label, selected = selected, size = s, tint = tint, onClick = onClick) },
    onClick = onClick,
)

/** How many of the optional actions fit beside the fixed buttons in [width]. */
fun barSlots(width: Dp, button: Dp, fixedButtons: Int): Int {
    // Both bars' insides and the row's outer padding.
    val spare = width - 16.dp - 8.dp - 8.dp - 8.dp - button * fixedButtons
    return (spare / button).toInt().coerceAtLeast(0)
}

@Composable
fun MoreMenu(actions: List<BarAction>, size: Dp) {
    var open by remember { mutableStateOf(false) }
    Box {
        RoundButton(Icons.Outlined.MoreVert, "More", selected = open, size = size) { open = true }
        DropdownMenu(open, { open = false }, shape = RoundedCornerShape(16.dp), containerColor = AquaColors.PanelSolid) {
            actions.forEach { a ->
                DropdownMenuItem(
                    text = { Text(a.label, color = if (a.selected) AquaColors.Accent else AquaColors.Text, fontSize = 15.sp) },
                    leadingIcon = a.menuIcon,
                    onClick = {
                        open = false
                        a.onClick()
                    },
                )
            }
        }
    }
}

/** The sync button's colour: dim when off, red when the last push failed. */
fun syncTint(on: Boolean, primary: PushState, backup: PushState): Color? = when {
    !on -> null
    primary is PushState.Failed && backup !is PushState.Done -> AquaColors.Danger
    else -> AquaColors.Accent
}

/** Where paintings are pushed: a daemon, and a backup that gets each save five seconds later. */
@Composable
fun SyncDialog(settings: Settings, sync: Sync, onToggle: (Boolean) -> Unit, onSyncNow: () -> Unit, onDismiss: () -> Unit) {
    var on by remember { mutableStateOf(settings.syncOn) }
    var primary by remember { mutableStateOf(settings.syncPrimary) }
    var backup by remember { mutableStateOf(settings.syncBackup) }
    var key by remember { mutableStateOf(settings.syncKey) }
    var test by remember { mutableStateOf<String?>(null) }
    val status by sync.status.collectAsState()
    val scope = rememberCoroutineScope()
    fun store() {
        settings.syncPrimary = primary
        settings.syncBackup = backup
        settings.syncKey = key
    }
    AlertDialog(
        onDismissRequest = { store(); onDismiss() },
        title = { Text("Sync") },
        text = {
            Column {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text("Push every save", color = AquaColors.Text)
                        Text("to an aqua-sync daemon on your network", color = AquaColors.TextDim, fontSize = 12.sp)
                    }
                    Switch(on, { on = it; store(); settings.syncOn = it; onToggle(it) })
                }
                Spacer(Modifier.height(10.dp))
                val field = Modifier.fillMaxWidth()
                OutlinedTextField(primary, { primary = it }, field, label = { Text("Address") }, placeholder = { Text("192.168.1.20") }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri))
                StatusLine(status.primary)
                OutlinedTextField(backup, { backup = it }, field, label = { Text("Backup address") }, placeholder = { Text("optional") }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri))
                StatusLine(status.backup, "gets each save 5 s after the first")
                OutlinedTextField(key, { key = it }, field, label = { Text("Key") }, placeholder = { Text("if the daemon has one") }, singleLine = true)
                Spacer(Modifier.height(8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
                    TextButton(onClick = {
                        store()
                        test = "Checking…"
                        scope.launch {
                            test = withContext(Dispatchers.IO) {
                                listOf(primary, backup).filter { it.isNotBlank() }.joinToString("\n") { a ->
                                    runCatching { "$a: ${Sync.hello(a)}" }.getOrElse { e -> "$a: ${e.message ?: "no answer"}" }
                                }.ifEmpty { "Type an address first." }
                            }
                        }
                    }) { Text("Test") }
                    TextButton(onClick = { store(); onSyncNow() }, enabled = primary.isNotBlank() || backup.isNotBlank()) { Text("Sync now") }
                }
                test?.let { Text(it, color = AquaColors.TextDim, fontSize = 12.sp) }
            }
        },
        confirmButton = { TextButton(onClick = { store(); onDismiss() }) { Text("Done") } },
    )
}

@Composable
private fun StatusLine(s: PushState, idle: String = "") {
    val (text, color) = when (s) {
        PushState.Idle -> idle to AquaColors.TextDim
        PushState.Sending -> "Sending…" to AquaColors.TextDim
        is PushState.Done -> "Synced at ${DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(s.at))}" to AquaColors.Accent
        is PushState.Failed -> s.message to AquaColors.Danger
    }
    Text(text, color = color, fontSize = 12.sp, modifier = Modifier.height(20.dp))
}
