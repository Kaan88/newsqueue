# Mechanics sheet

This sheet records the values, formulas and rules measured from the original
program and used by the clone. The on-disk program is a 32-bit Windows Delphi
application packed with UPX; its loader stub is at `0xa6f7b0`. The real code
was recovered from a read-only runtime capture of the unpacked image
(`checks/original_memory.bin`), opened in Deep mode as a mapped PE image with
image base `0x400000`. The addresses below are 32-bit virtual addresses in
that unpacked image. Ghidra 12.1.4 produced the pseudo-C records; only the
measured values and control flow are quoted here, not source code.

## Function map

| Address | Role | Measured behaviour |
|---|---|---|
| `0x817178` | `SetspeedLimit` | Computes the global limit from the menu tag, stores the per-connection limit, updates the caption and applies it to every connection. |
| `0x8340ec` | Value-to-label formatter | Selects one of nine display modes; divides by `1024`, `1048576`, `1000` or `1000000` and multiplies by `8` for bit rates. |
| `0x834530` | Speed panel | Formats the current speed with `SpeedBytesMode` and updates the speed panel. |
| `0x833ffc` | Main timer (`OnMaintimer`) | Accumulates elapsed milliseconds and runs the 1,000 ms, 60,000 ms and 3,600,000 ms gates. |
| `0x8345d4` | One-second transfer worker | Computes bytes/elapsed seconds, keeps a four-sample average, calibrates the Auto speed ceiling and updates ETA, speed and transfer panels. |
| `0x835e38` | Apply per-connection limit | Writes `DAT_008a4b6c` (global limit divided by active connections) to every connection object. |
| `0x8060f0` | Byte/speed unit formatter | Divides by `1024`, `1048576`, `1073741824` or `1099511627776`; uses the Delphi format strings `#,#0`, `#,#0.0` and `#,#0.00`. |
| `0x80dc2c` | Queue-size panel | Formats the queue total with `QueueBytesMode`, two decimals, and writes the queue panel. |
| `0x80d954` | Free-space panel | Compares free MB with `LowFreeSpace`; logs the exact error, pauses downloads and displays free space in GB. |
| `0x809f1c` | ETA formatter | Splits whole seconds into days, hours, minutes and seconds and joins the non-zero parts. |
| `0x836048` | ETA/transfered/status update | ETA = remaining bytes / current speed; transfered display mode `0` maps to MB, `1` to GB, `2` to TB. |
| `0x835918` | Connection and thread limits | Counts connections, enforces 200 connections per thread and 8 servers per level, and reports the limit message. |
| `0x870648` | `serveredit_sslClick` | Sets the port edit to `563` when SSL is checked and `119` when it is cleared. |
| `0x870694` | `serveredit_save_buttonClick` | Accepts only when name and address are non-empty and the port text is not `"0"`. |
| `0x8183f4` | Configuration save | Writes the measured settings keys and current values to the configuration. |
| `0x80e294` | Connection list refresh | Rebuilds the connection/thread rows from the active connection list. |

## Identity

| Value | Source |
|---|---|
| Version 0.48.4.0, product 0.48.4 | `RT_VERSION`, rva `0x680868` |
| Build date 2024-10-07 | `log/20261103.log` first line |
| Packed entry point `0xa6f7b0` (LZMA unpacker) | `capture/README.md`, Ghidra `0xa6f7b0` |
| Main form design size 1490 x 668 client pixels | main form DFM, rva `0x57c7f0` |
| Saved window 1285 x 881, normal state | `misc/config.abz` `[Main form]` |
| UI font Microsoft Sans Serif, 11 px (8.25 pt) | main form DFM `Font.*` |

## Window chrome

| Value | Source |
|---|---|
| Main toolbar height 55 px | main form DFM, `maintoolbar.Height` |
| Toolbar button 55 x 53 px, icon above caption | main form DFM, `mtb_*` buttons |
| Toolbar group separators 2 px | main form DFM, `spacer_mali*` |
| Tab strip height about 26 px | main form DFM + runtime screenshot |
| Download-queue secondary toolbar 30 px | main form DFM, `DQDock.Height` |
| Connection panel default height 187 px; saved 278 px | main form DFM; `[UIMisc] ThreadListHeight=278` |
| Status bar 21 px | main form DFM, `mainstatusbar.Height` |
| Main tabs: Download queue, PAR2, Explorer, Log, RSS | main form DFM + `[Tabs]` |
| Tab visibility: queue, PAR2, XPAT, NGS, log, explorer, RSS all on | `[Tabs] *TabVisible=1` |
| Queue tab index 0, PAR2 1 | `[Tabs] DQTabIndex=0`, `PAR2Tabindex=1` |

## Download queue columns

Column names come from the English language table; widths and visibility come
from the saved configuration. Values are pixels. Source for all rows:
`Lang.en.abz` `[TAltBinzMainForm] downtree.Header.Columns[n].Text` and
`misc/config.abz` `[Queue tree] ColumnNWidth` / `ColumnNVisible`.

| # | Column | Width | Visible |
|---|---|---|---|
| 0 | Subject | 390 | yes |
| 1 | Size | 95 | yes |
| 2 | Parts | 95 | yes |
| 3 | Age | 95 | yes |
| 4 | Group | 95 | yes |
| 5 | Poster | 151 | yes |
| 6 | Status | 95 | yes |
| 7 | Download folder | 220 | yes |
| 8 | Unrar | 38 | yes |
| 9 | Execute | 95 | yes |
| 10 | Delete archives | 50 | no |
| 11 | Posted on | 50 | no |

Other queue rules:

- Groups are expandable collections; files are children. Source: queue tree in
  the main form DFM and the queue serialization (`*.abz`).
- Double click toggles pause/unpause unless
  `[UIMisc] Dblclicktogglesexpcoll=1`. Saved value is `0`.
- Remove completed entries with Cleanup: `[UIMisc] CleanupQueue=1`.
- Autosave the queue every 5 minutes: `[UIMisc] Savequeue=1`,
  `Savequeuetime=5`.
- Maximum download path length 200 characters: `[UIMisc] Max_Path_Length=200`.
- Skip a file that already exists in the download folder:
  `[UIMisc] SkipFileIfExists=1`.
- Queue size is the sum of every collection's total bytes; completed entries
  remain in the sum until Cleanup removes them (function `0x80dc2c`).

## Status bar

| Panel | Width | Saved display mode | Source |
|---|---|---|---|
| Speed limit | 90 | `No limit`, otherwise the formatted global limit | `Lang.en.abz` `SBLimitWidth`, `msb_speedlimit`, function `0x817178` |
| Current speed | 90 | Mbit/s (`SpeedBytesMode=3`) | `SBSpeedWidth`, `[UIMisc]`, functions `0x8340ec` / `0x834530` |
| Speed graph | 120 | 2-minute graph, one sample per second | main form DFM `msb_speedgraph32`; function `0x8345d4` |
| Transfered | 100 | MB (`TransferedBytesMode=0` maps to the MB formatter) | `SBTransferedWidth`, function `0x836048` |
| ETA | 100 | `Unknown` at zero speed | `SBETAWidth`, functions `0x809f1c` / `0x836048` |
| Queue size | 100 | MB (`QueueBytesMode=1`) | `SBQueueWidth`, function `0x80dc2c` |
| Free space (download) | 100 | GB | `SBFreeDownloadWidth`, function `0x80d954` |
| Free space (unrar) | 100 | GB | `SBFreeUnrarWidth`, function `0x80d954` |
| Spring / thread toggle | remainder | 20 px thread button | main form DFM |

Formatting rules from `Lang.en.abz` `[constants]` and the formatter functions:

- Speed: `%s KiB/s`, `%s MiB/s`, `%s kbit/s`, `%s Mbit/s`, `%s kB/s`,
  `%s MB/s` (functions `0x8340ec`, `0x834530`).
- Bytes: `%s kB`, `%s MB`, `%s GB`, `%s TB` (functions `0x8060f0`,
  `0x80dc2c`).
- Divisors: `1024`, `1048576`, `1073741824`, `1099511627776` for the 1024-based
  units; `1000` and `1000000` for decimal kB/MB; multiply by `8` for bits
  (constants at `0x8068e0`, `0x806914`, `0x806918`, `0x80691c`, `0x83451c`,
  `0x834520`, `0x834524`, `0x834528`, `0x83452c`).
- Decimal places: speed modes 0 and 4 and byte mode 0 use `#,#0` (no
  decimals); the other fixed modes use `#,#0.00` (two decimals). The queue
  panel passes two decimals. Format strings at `0x8068ec`, `0x8068fc`,
  `0x80690c`.
- Time parts: `d`, `h`, `m`, `s`; the ETA formatter joins every non-zero part,
  for example `1d 2h 3m 4s`. ETA is `Unknown` when the current speed is zero
  (`0x809f1c`, `0x836048`).
- Free space under the configured threshold logs
  `Error! Free space on destination drive is under configured value. Downloading will be paused.`
  followed by `Downloading paused.` and pauses: function `0x80d954`;
  `[Basic] LowFreeSpace=1000` MB.

## Connections and servers

| Value | Source |
|---|---|
| Number of connections on the saved server: 8 | `misc/servers.abz` offset `0x30` |
| SSL enabled on the saved server, port 563 | `misc/servers.abz`; function `0x870648` |
| SSL default port 563; plain default port 119 | `Function 0x870648 serveredit_sslClick`; strings `563` / `119` at `0x870684` / `0x870690` |
| Maximum connections per server 200 | `Lang.en.abz` `server_numconnection.Hint` (`[1-200]`); function `0x835918` |
| Maximum 8 servers per level | `Lang.en.abz` `str_server_level_max`; function `0x835918` |
| Maximum 200 connections per download thread | function `0x835918` error text |
| Connections per thread 1 | `[Connection] ConnectionsPerThread=1` |
| Maximum download speed 55 KB/s (0.44 Mbit/s) | `[Connection] MaxSpeed=55`, `MaxSpeedMbps=0.439999997615814`; hint `[0-10000] 0-Auto` |
| Connection retry delay 10 s | `[Connection] ConnectionRetryDelay=10` |
| Connection timeout 30 s | `[Connection] ConnectionTimeout=30` |
| XPAT timeout 300 s | `[Connection] XPATConnectionTimeout=300` |
| Retry count 0, retry delay 5 s | `[Connection] RetryCount=0`, `RetryDelay=5` |
| Connected while idle on | `[Connection] ConnectedWhileIdle=1` |
| Grace period 0 min | `[Connection] GracePeriod=0` |

Server editor fields (from the server-edit DFM, rva `0x628188`): Name, Address,
Port, Number of connections, Select group, ARTICLE command, SSL, Connected while
idle, Server requires authentication (Username/Password), Server purpose
(Level 0 Primary), Max age, Network adapter. Save accepts only when name,
address and port are non-empty and port is not `0`
(`Function 0x870694 serveredit_save_buttonClick`). The clone also uses
`0x870648`'s rule when the SSL box changes, and its downloader opens a TLS
connection for an SSL server and a plain NNTP connection otherwise.

## Transfer maths

- 100% reference speed is 55,000 bytes/s (0.44 Mbit/s). Source:
  `[Connection] MaxSpeed=55` KB/s, `MaxSpeedMbps=0.439999997615814`.
- Speed limit menu (function `0x817178`):
  `limit = trunc(tag * MaxSpeedMbps / 10.0 * 125000.0)`. At the measured
  `MaxSpeedMbps=0.44` this is exactly `tag * 5,500` bytes/s. Tags 1..9 are
  10%..90%; tag 10 is the 100% value, 55,000 bytes/s. Tag 0 is `No limit` and
  stores 0 (unthrottled), not a 55,000-byte/s cap. Constants `10.0` at
  `0x817264`, `125000.0` at `0x817268`, reference at `0x8ca32c`.
- Per-connection limit: the global limit is divided by the number of active
  connections when that count is greater than zero, otherwise the
  per-connection value is 0. Same function `0x817178`; the divisor is
  `DAT_008ca178` (active connections) and the result is `DAT_008a4b6c`.
  Function `0x835e38` writes that per-connection value to every connection
  object.
- Maximum speed `0` in Setup is Auto (hint `[0-10000] 0-Auto`). During the
  first 60 one-second ticks, function `0x8345d4` raises the `0x8ca32c`
  reference to the highest observed `bytes/s / 125000` (Mbit/s), then stops.
- Current speed is bytes transferred divided by elapsed seconds. The main
  timer `0x833ffc` accumulates elapsed milliseconds and calls `0x8345d4` at
  the 1,000 ms gate. `0x8345d4` keeps four 1-second samples and displays
  `sum / 4`; constants `1000.0` at `0x834e5c` and `4.0` at `0x834e60`.
  The minute statistics gate is 60,000 ms and the hourly gate is 3,600,000 ms
  (same function `0x833ffc`).
- ETA = remaining bytes / current speed; shown as `Unknown` when the current
  speed is zero. Functions `0x836048` and `0x809f1c`; remaining-byte globals
  `0x8a4b58` / `0x8a4b48`.
- Queue size = sum of the total bytes of all queue collections (completed
  entries stay in the total until Cleanup removes them); formatted with
  `Function 0x80dc2c Updatequeuesizepanel` and the byte formatter at
  `0x8060f0`. The panel uses `QueueBytesMode` (saved `1` = MB) and two
  decimals.
- Transfered panel (function `0x836048`): saved `TransferedBytesMode=0`
  selects the MB formatter; `1` selects GB and `2` selects TB. It is not the
  automatic unit mode.
- Free space is queried in MB, compared with `LowFreeSpace=1000` MB, and
  displayed in GB by dividing by `1024.0` (constants `0x80dc08` and
  `0x8ca338`; function `0x80d954`).

### Measured constants

| Address | Value | Use |
|---|---|---|
| `0x817264` | 10.0 | speed-limit percentage divisor |
| `0x817268` | 125000.0 | bytes/s per Mbit/s |
| `0x8ca32c` | 0.44 | 100% reference in Mbit/s (runtime, saved `MaxSpeedMbps`) |
| `0x83451c` | 1024.0 | KiB divisor |
| `0x834520` | 1048576.0 | MiB divisor |
| `0x834524` | 1000.0 | decimal kB divisor |
| `0x834528` | 8.0 | bits per byte |
| `0x83452c` | 1000000.0 | decimal MB / Mbit divisor |
| `0x834e5c` | 1000.0 | milliseconds per second |
| `0x834e60` | 4.0 | speed samples in the moving average |
| `0x834e64` | 125000.0 | bytes/s to Mbit/s |
| `0x834e68` | 1250.0 | percent-of-reference scaling (taskbar path) |
| `0x8068e0` | 1024.0 | kB divisor |
| `0x806914` | 1048576.0 | MB divisor |
| `0x806918` | 1073741824.0 | GB divisor |
| `0x80691c` | 1099511627776.0 | TB divisor |
| `0x80dc08` | 1024.0 | free-space MB to GB |
| `0x8ca338` | 1000 | `LowFreeSpace` in MB |

## Logging, history and statistics

| Value | Source |
|---|---|
| Logging enabled, to file | `[Log] LogEnabled=1`, `LogToFile=1` |
| Keep 7 days of log files | `[Log] Keeplastdays=7` |
| Maximum 200 visible log lines | `[Log] MaxLines=200` |
| Detailed log, download history, speed and bytes | `[Log] DetailedLog=1`, `DownloadHistory=1`, `SpeedAndBytes=1` |
| History retention 0 days (keep all) | `[Basic] DownloadHistoryDays=0` |
| Clear history on exit off | `[Basic] ClearHistoryOnExit=0` |
| Statistics tabs: Daily transfer, Monthly transfer, Daily speedgraph, Download history, Misc. stats | `[TStatsForm]` language table |

## Scheduler, PAR2 and unpacking

| Value | Source |
|---|---|
| Scheduler: 7 days x 48 half-hour slots | `[Schedule] Mon..Sun` strings of 48 characters |
| Saved pattern: `1010...` (alternating slots) | `[Schedule] Mon..Sun` |
| PAR2 checking and repairing on | `[PAR2] PAR2checking=1`, `PAR2Repairing=1` |
| Cleanup after PAR2 repair on | `[PAR2] CleanupAfterPAR2Repairing=1` |
| Delete small PAR2 file off | `[PAR2] DeleteSmallPAR2file=0` |
| Save PAR2 info on exit on | `[PAR2] SavePAR2info=1` |
| PAR2, decode, download thread priority 3 | `[Basic] Par2ThreadPriority=3`, `DecodeThreadPriority=3`, `DownloadThreadPriority=3` |
| Autounpack off | `[Unrar] AutoUnrar=0` |
| Overwrite existing files off | `[Unrar] OverwriteExistingFiles=0` |
| Delete archives after unpack off | `[Unrar] DeleteUnraredFiles=0` |
| Strip root folder on | `[Unrar] RemoveLevel1Dir=1` |
| Unrar rename instead of overwrite on | `[Unrar] RenameExistingFiles=1` |

## Unrar

The original loads `unrar.dll` dynamically and resolves the RAR 3.x API
(`RAROpenArchive`, `RAROpenArchiveEx`, `RARCloseArchive`, `RARReadHeader`,
`RARReadHeaderEx`, `RARProcessFile`, `RARProcessFileW`, `RARSetCallback`,
`RARSetChangeVolProc`, `RARSetProcessDataProc`, `RARSetPassword`,
`RARGetDllVersion`) in function `0x5b6148`. It requires DLL version 4 or newer
and shows `please download the newest "unrar.dll" file...` otherwise.

| Address | Role | Measured behaviour |
|---|---|---|
| `0x5b6048` | Unrar component constructor | Loads the configured DLL through `0x5b6148`. |
| `0x5b6750` | Open archive | Opens one archive and reads its first header. |
| `0x5b68a4` | Extract archive | Extracts to the configured destination; uses `0x5b6f4c`. |
| `0x5b6f4c` | RAR process loop | Calls `RARProcessFileW` for every header and applies overwrite / password flags. |
| `0x85b32c` | Build unpack paths | Reads the collection source folder and unrar folder, creates the target directory and logs `Failed creating unrar dir:`. |
| `0x85b85c` | Unrar worker thread | Builds the volume list, extracts, then moves/deletes configured files. Logs `unrar started`, `unrar finished` and `unrar failed`. |
| `0x82ea7c` | Queue `Unrar` button | Sets the per-collection unrar flags (`+0x8c` and `+0x84`); it does not start a worker itself. |

Volume selection in `0x85b85c` accepts `*.001`, `*.part1.rar`, `*.part01.rar`,
`*.part001.rar` and plain `*.rar`; later `partN` volumes and `.r00` volumes are
not treated as the first archive. The worker then extracts the selected
archive; UnRAR itself follows the remaining volumes.

`AutoUnrar` is described in the language table as "Collections will be
autounpacked after par2 checking" and is off in the saved profile. The clone
does not run a real PAR2 check, so it starts auto-unpack when the download
reaches `Completed`. The queue button and context menu provide a manual
`Unpack now` path as well.

The original ships 32-bit `unrar.dll` / `UnRAR.dll` files. The clone is a
64-bit process, so it runs the standalone 64-bit `UnRAR.exe` from `app/tools`
(or an installed WinRAR) instead; the extraction flags and post-processing are
the same.

## Explorer tab

The original Explorer form is `TExplorerform` (DFM rva `0x60e77c`):

| Part | Measured layout |
|---|---|
| Left tree | `TVirtualExplorerTreeview`, 150 px wide at design time; saved `ExpFoldersWidth=250`. |
| Right list | `TVirtualExplorerListview` with columns Name, Size, Type, Modified, Attributes, Accessed, Created, DOS Name, Path. Each column is 200 px; `[Exp tree]` makes the first five visible and the last four hidden. |
| Toolbar | 62 px high: Back, Forward, Up, Refresh, Folders toggle, 298 px shell combo, 122 px filter; a second row has Add/Remove favourite. |
| Refresh | `0x837a5c` calls the tree/list refresh methods. |
| Folders toggle | `0x837a7c` hides/shows the tree and splitter. |
| Add favourite | `0x837b20` opens a name dialog and adds the current folder. |

Double-click, shell context menus, rename, delete and drag/drop are handled
inside the VirtualShellTools controls. The clone implements the equivalent
file-list selection, double-click navigation/open, context menu, history,
favourites and column visibility directly with egui.

## Keyboard shortcuts

From the language table `ShortCut` values (Delphi `ShortCut` codes) and the main
form:

| Key | Action |
|---|---|
| F1 | Pause / resume downloads |
| F2 / F3 | Previous / next tab |
| F9 | Show or hide the connections panel |
| Ctrl+F | Global search |
| F4 | Focus the search box |
| Ctrl+P | Pause selected files |
| Ctrl+D | Unpause selected files |
| Ctrl+Up / Ctrl+Down | Move selected up / down |
| Ctrl+PageUp / Ctrl+PageDown | Move selected to top / bottom |
| Ctrl+E | Export selected to NZB |
| Ctrl+H | Change download folder |
| Ctrl+R | Rename collection |
| Ctrl+S | Split |
| Ctrl+J | Join |
| Ctrl+T | ETA on selected |
| Delete | Delete selected |

## Scope and estimated items

- The clone reproduces the observed queue, transfer, connection and server
  configuration, settings pages, categories, statistics, notes, search, log,
  PAR2 list and scheduler.
- Downloads are real: the queue worker opens NNTP connections to the enabled
  server, authenticates with the saved account, fetches articles with
  `BODY <message-id>`, undoes NNTP dot-stuffing, decodes yEnc and writes the
  assembled files into the collection's download folder. The number of
  parallel connections is the enabled server's connection count (the saved
  profile uses 8), and the global speed limit is divided across them as
  function `0x835e38` does. Pause, disconnect, speed limit and progress use
  the same configured values as the rest of the window. The workers use
  plain NNTP for an SSL-off server and TLS for an SSL-on server; the saved
  profile is the original's 8-connection SSL profile on port 563, and both
  that profile and a plain 119 profile were exercised end to end. PAR2 repair
  remains out of scope. RAR extraction runs the 64-bit `UnRAR.exe` in
  `app/tools` (the original's bundled DLL is 32-bit), with overwrite/rename,
  strip-root, password, delete-archive, move/delete-pattern and different-folder
  handling. These are scope limits, not measured values.
- The exact skin artwork, icons and the 55-pixel toolbar images are not copied.
  Simple vector glyphs in the measured palette are used instead (estimated
  visual placeholder).
- The 561 KB main form's runtime-created tab contents are only partly present in
  the DFM; the Download queue and PAR2 trees are present, while the Explorer,
  Log, RSS and search tabs are created in code. Their column names and controls
  were taken from the language table (observed) and laid out to match the
  measured main-window proportions (estimated layout).
- Sample queue names, article counts and progress values are demonstration data,
  not application constants.
