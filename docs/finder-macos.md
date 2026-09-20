# macOS Finder setup and acceptance

This procedure records behavior observed on macOS 27.0 build 26A428 with macFUSE 5.4.0 and RWS SSHFS `3.7.5-rws-fskit3`. Labels can differ by macOS version or language. It is not a claim of compatibility with every Mac. Build and mount using [the prototype guide](prototype.md) and [patched SSHFS instructions](sshfs-fskit.md) first.

## Automatic pinning in the RWS app

The app's **Ajouter** action registers the workspace, connects it, and pins the
mounted folder under Finder **Favourites**. **Ouvrir dans le Finder** also refreshes
this favorite after a successful connection. A failed connection never pins an
unmounted path; missing prerequisites leave the space registered with an explanation.
A pinning failure is reported separately while the mounted folder still opens.

This uses Apple's deprecated shared-file-list API, isolated behind a small native
bridge, with read-back verification. Repeated pinning of the same mounted URL was
verified to create only one favorite. Other favorites are preserved. An existing
entry under Locations may still be visible as well; RWS does not remove it.
Disconnect removes the managed favorite after successfully unmounting. Reconnect
removes the prior bookmark and creates a fresh one: re-inserting a URL alone is
insufficient when FSKit changes the volume identity. A managed-path property
identifies RWS entries even after their volume disappears. Earlier unresolved
favorites are migrated only when their exact display name matches the current RWS
volume name. Other favorites are not removed.

The live CLI/helper disconnect/remount cycle and subsequent Finder clicks passed
on the tested Mac, including navigating away and reopening the favorite. A favorite
does not initiate a connection when the workspace is disconnected: reconnect in
RWS first. See the current validation log.

## Manual fallback: show the mounted volume in the left sidebar

1. Open Finder > Settings (Réglages) > Sidebar (Barre latérale).
2. Under Locations (Emplacements), inspect **Connected servers**, **External disks**, and **Hard disks** (Serveurs connectés, Disques externes, Disques durs). Record the initial checkbox states. A mixed checkbox is not fully enabled. If the user wants these categories visible, enable them; this may show other disks too. An agent needs authorization for preference changes unless already provided in the current setup session.
3. If the entire sidebar is hidden, use View > Show Sidebar (Présentation > Afficher la barre latérale). Expand Locations if collapsed.
4. Open Go > Computer (Aller > Ordinateur), or **⇧⌘C**. Select the mounted `RWS-WORKSPACE` volume, then File > Add to Sidebar (Fichier > Ajouter à la barre latérale).
5. Click the new entry under Locations. Confirm that the expected remote contents appear and an eject control is available. Seeing the volume icon alone is not an acceptance test.

On the tested Mac, Connected servers was already enabled; Hard disks and External disks were mixed. Enabling both disk categories did **not** automatically add RWS. Adding the selected volume explicitly produced the working Locations entry. RWS is a remote mounted filesystem; it is not a physical disk and is not forced into local-disk mode. No Full Disk Access grant is needed for this Finder setup.

These are per-user Finder preferences. RWS does not change these display categories automatically. Restore their recorded previous states if the user asks to undo the display changes. Removing a sidebar shortcut does not unmount the filesystem or delete remote files; ejecting does unmount it.

## After an unmount/remount

The sidebar entry can disappear or remain as a stale shortcut. In the latter case, Finder may say the original item cannot be found even though the new volume works by path.

1. Check the current mount by opening its actual path with **⇧⌘G**, for example `/Volumes/RWS-demo`.
2. If that works but the sidebar entry fails, right-click the stale entry and choose **Remove from Sidebar**. Do not select an eject or delete action.
3. Add the current volume from Computer using the procedure above, then test clicking it again.

Automatic sidebar persistence remains unresolved. Each observed FSKit mount received a different mount URL identifier; this is evidence to investigate, not proof of the exact Finder bookmarking mechanism.

## Validate real Finder operations

Use an authorized disposable test directory and unique names. Do not test destructive actions against existing user documents.

1. Open the volume from its sidebar entry, leave it for Computer, and return several times. Check both the root and a child directory. The list must remain populated.
2. Use File > New Folder or **⇧⌘N**. Confirm that the new directory appears, then independently check its existence on the remote host through SSH or `rws exec`.
3. Copy a small, uniquely named local text file in Finder (**⌘C**, then **⌘V** in the mounted directory). Confirm its appearance and exact remote contents. Finder has no general New File command: create files from an editor's Save dialog or copy existing files.
4. Open the copied file in a local editor, modify and save it, then compare the remote bytes. Repeat with accents/emoji, rename, and replacement of a disposable file. **The editor step is still pending in the recorded validation; Finder copy/paste alone does not prove editor save compatibility.**
5. Run `python3 scripts/test-mount-listing.py /Volumes/RWS-demo` against the authorized mount root. It repeatedly creates/deletes Unicode test files and checks root and child listings. It removes its unique test directory on success and retains it on failure.
6. Leave the mount in all terminals, close edited files, unmount normally, and verify the OS mount entry and SSHFS process have gone. For a retained demo, record that unmount was deferred.

Changes made outside Finder may require leaving and reopening the directory to refresh the view. Live remote-change notifications and their timing have not been validated.

## Distinguish common failures

| Symptom | Evidence to collect | Next action |
| --- | --- | --- |
| No sidebar entry, but mounted path works | OS mount entry and Computer view | Configure display categories as authorized; add the actual volume |
| Sidebar says original item cannot be found | Actual path works after remount | Replace the stale shortcut |
| First root listing works, later listings are empty; mkdir appears ineffective | Compare repeated local listings with SSH; check selected binary | Rebuild/select `3.7.5-rws-fskit3`; run the live listing test |
| Accented names give EEXIST/ENOENT or disappear | Compare remote filename normalization and binary version | Use the patched build and read its NFC remote-name contract; do not rename remote data automatically |
| Mount command reports success but Finder is empty | Actual mount, repeat listings, remote listing, private SSHFS log | Do not infer success from an icon, process exit code, or a one-time subdirectory read |
| SSH works but mounted reads hang | Mount entry, process state, SSHFS log, bounded diagnostic command | Preserve evidence and stop piling up blocked probes; normal unmount first |

A diagnostic SSHFS crash left OS I/O and unmount processes blocked in one intermediate test. A forced unmount removed the mount entry but did not establish process cleanup. Do not automatically restart all FSKit services, force-unmount other volumes, or reboot a user's Mac. Record outstanding processes and coordinate recovery around active work. The final corrected Documents mount was responsive; general crash/network recovery is not validated.

## Evidence and screenshots

Keep raw screenshots, hostnames, actual paths, process IDs and logs in ignored `.rws-local/diagnostics/`. Record what each capture proves. Useful milestones are sidebar settings, root contents after clicking the sidebar, a Finder-created directory/file, and remount behavior. Review and anonymize images before publishing them as README assets. Current originals are private; the public [validation record](validation.md) summarizes their results.
