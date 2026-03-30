# Peekabrowser - macOS Sidebar Browser

**[繁體中文](#繁體中文) | [English](#english)**

---

## 繁體中文

Peekabrowser 是一個輕量級的 macOS 側邊欄瀏覽器，讓你快速存取 Google、ChatGPT、Claude、Gemini、Perplexity 等搜尋引擎和 AI 服務。它常駐在螢幕左側，隨時可以呼叫使用。

### 安裝方式

1. 開啟 `Peekabrowser_1.4.0_aarch64.dmg`
2. 將 `Peekabrowser.app` 拖曳到「應用程式」資料夾
3. 第一次開啟時，macOS 會提示「無法打開 Peekabrowser，因為它來自未識別的開發者」
4. 前往 **系統設定 → 隱私與安全性**，找到 Peekabrowser 的提示，點擊 **「仍要打開」**
5. 再次開啟 Peekabrowser，點擊 **「打開」** 確認

> **注意：** Peekabrowser 目前沒有 Apple 開發者簽名，所以需要手動允許。這不會影響功能或安全性。

### 系統需求

- macOS 13.0 (Ventura) 或更新版本
- Apple Silicon Mac

### 基本使用

**啟動**

啟動後，Peekabrowser 會在選單列 (Menu Bar) 顯示一個圖示。它不會出現在 Dock 上。

**顯示側邊欄**

| 方式 | 說明 |
|------|------|
| 快捷鍵 | 按下 `⌘⇧A` (Command+Shift+A) |
| 滑鼠 | 將滑鼠移到螢幕最左邊緣，停留約 0.3 秒 |
| 選單列 | 點擊選單列的 Peekabrowser 圖示 |

**切換 Destination**

側邊欄左側有一排圖示按鈕，每個代表一個 Destination（目的地）。點擊圖示即可開啟對應的網站。

預設的 Destination：

| 名稱 | 網址 |
|------|------|
| Google | google.com |
| ChatGPT | chat.openai.com |
| Claude | claude.ai |
| Gemini | gemini.google.com |
| Perplexity | perplexity.ai |
| OpenEvidence | openevidence.com |

| Calendar | system://calendar |
| Reminders | system://reminders |

圖示預設會自動顯示網站的 favicon。也可以在設定中自訂 emoji 作為圖示。

**分頁管理**

- 點擊同一個 Destination 圖示會開啟該服務的頁面
- 每個 Destination 下方會出現編號小圓點，代表該服務的分頁
- 點擊小圓點可切換分頁
- 按 `⌘W` 關閉當前分頁
- 滑鼠移到小圓點上，會出現 ✕ 按鈕可關閉該分頁

**調整寬度**

側邊欄底部有 S / M / L 三個按鈕：

| 模式 | 寬度 | 高度 |
|------|------|------|
| S | 螢幕 1/3 | 50% |
| M | 螢幕 1/2 | 70% |
| L | 螢幕 2/3 | 85% |

### 快捷鍵

**全域快捷鍵（任何應用程式中都可使用）**

| 快捷鍵 | 功能 |
|--------|------|
| `⌘⇧A` | 顯示/隱藏側邊欄 |
| `⌘⇧S` | 螢幕截圖並傳送給 AI |
| `⌘C` `⌘C` | 快速複製兩次，選擇要傳送到哪個 Destination |

**側邊欄內快捷鍵**

| 快捷鍵 | 功能 |
|--------|------|
| `⌘N` | 開啟新分頁 |
| `⌘R` | 重新載入當前頁面 |
| `⌘W` | 關閉當前分頁 |

**自定義快捷鍵**

1. 點擊側邊欄底部的 ⚙ 設定按鈕
2. 在「Keyboard Shortcuts」區塊中，點擊你想修改的快捷鍵
3. 該欄位會顯示「Press keys...」並閃爍
4. 按下你想要的新快捷鍵組合
5. 快捷鍵立即生效並自動儲存

### 進階功能

**快速傳送剪貼簿 (⌘C ⌘C)**

1. 選取要傳送的文字
2. 快速連按兩次 `⌘C`（在 0.5 秒內）
3. 螢幕上會彈出 Destination 選擇器
4. 點擊要傳送到的目的地
5. Peekabrowser 會自動開啟對應頁面並將文字貼入輸入框

**螢幕截圖傳送 (⌘⇧S)**

1. 按下 `⌘⇧S`
2. 側邊欄會自動隱藏
3. 使用系統截圖工具選取螢幕區域
4. 截圖完成後，會彈出 Destination 選擇器
5. 選擇要傳送到的 AI 服務
6. 截圖會自動貼入該服務的輸入框

> **注意：** 首次使用需要在 **系統設定 → 隱私與安全性 → 螢幕錄製** 中允許 Peekabrowser 的權限。

**雙螢幕支援**

Peekabrowser 支援多螢幕環境。側邊欄會出現在滑鼠所在的螢幕上。

**行事曆與提醒事項整合 (v1.1.0)**

在 Destination 列表中加入 Calendar 和 Reminders 後：

- **點擊側邊欄圖示**：開啟 macOS 原生行事曆 / 提醒事項 App
- **⌘C ⌘C 傳送文字**：複製文字後快速連按兩次 ⌘C，選擇 Calendar 或 Reminders，會彈出設定視窗讓你：
  - 編輯標題
  - 選擇行事曆 / 提醒列表
  - 設定日期時間
- **截圖 OCR**：按 ⌘⇧S 截圖後選擇 Calendar 或 Reminders，系統會自動辨識圖片中的文字並填入標題欄位

**調整 Destination 排序 (v1.1.0)**

在設定頁面中，每個 Destination 旁邊有 ↑↓ 按鈕可調整顯示順序。

**新增分頁與預設瀏覽器開啟 (v1.2.0)**

側邊欄底部新增兩個按鈕：
- **+ (New Tab)**：為目前的 Destination 開啟新分頁，快捷鍵 `⌘N`
- **↗ (Open in Browser)**：用預設瀏覽器開啟目前正在瀏覽的頁面

### 設定

點擊側邊欄底部的 ⚙ 按鈕開啟設定：

**管理 Destination**

- **新增：** 點擊「+ Add Destination」，從預設列表選擇或點「+ Custom」自行填入名稱和 URL
- **編輯：** 點擊 Destination 右側的 ✎ 按鈕，可修改名稱、URL 和圖示
- **刪除：** 點擊 Destination 右側的 ✕ 按鈕
- **排序：** 點擊 ↑↓ 按鈕調整順序

**圖示設定**

- **自動抓取網站圖示：** Icon (emoji) 欄位留空時，會自動顯示該網站的 favicon
- **自訂 emoji：** 在 Icon (emoji) 欄位輸入 emoji，即可使用自訂圖示

### 資料儲存

所有設定資料儲存在本機：

```
~/Library/Application Support/com.peekabrowser.app/
├── destinations.json    # Destination 列表
└── shortcuts.json       # 快捷鍵設定
```

- 網站的登入狀態和 Cookie 會保留在 WebView 中
- 切換分頁不會遺失登入狀態
- 解除安裝 Peekabrowser 後，可手動刪除上述資料夾清除所有資料

### 退出

右鍵點擊選單列上的 Peekabrowser 圖示，選擇 **Quit Peekabrowser**。

### 疑難排解

**無法開啟 App**
前往 **系統設定 → 隱私與安全性**，找到 Peekabrowser 的提示並點擊「仍要打開」。

**螢幕截圖不能使用**
前往 **系統設定 → 隱私與安全性 → 螢幕錄製**，確認 Peekabrowser 已被允許。

**Google 登入失敗**
部分第三方登入服務可能在內嵌瀏覽器中受限，這是 Google 的安全政策限制。

**側邊欄沒有出現**
確認 Peekabrowser 正在執行（選單列有圖示），然後按 `⌘⇧A` 或將滑鼠移到螢幕最左邊。

---

## English

Peekabrowser is a lightweight macOS sidebar browser that gives you instant access to Google, ChatGPT, Claude, Gemini, Perplexity, and other AI services. It lives on the left edge of your screen and can be summoned at any time.

### Installation

1. Open `Peekabrowser_1.4.0_aarch64.dmg`
2. Drag `Peekabrowser.app` to the Applications folder
3. On first launch, macOS will show "Peekabrowser cannot be opened because it is from an unidentified developer"
4. Go to **System Settings → Privacy & Security**, find the Peekabrowser prompt, and click **"Open Anyway"**
5. Re-open Peekabrowser and click **"Open"** to confirm

> **Note:** Peekabrowser is not signed with an Apple Developer certificate, so this manual step is required. It does not affect functionality or security.

### Requirements

- macOS 13.0 (Ventura) or later
- Apple Silicon Mac

### Basic Usage

**Launching**

After launch, Peekabrowser appears as an icon in the Menu Bar. It does not show in the Dock.

**Showing the Sidebar**

| Method | Description |
|--------|-------------|
| Shortcut | Press `⌘⇧A` (Command+Shift+A) |
| Mouse | Move cursor to the far left edge of the screen and hover for ~0.3s |
| Menu Bar | Click the Peekabrowser icon in the Menu Bar |

**Switching Destinations**

The sidebar has a column of icon buttons on the left, each representing a Destination. Click an icon to open that website.

Default Destinations:

| Name | URL |
|------|-----|
| Google | google.com |
| ChatGPT | chat.openai.com |
| Claude | claude.ai |
| Gemini | gemini.google.com |
| Perplexity | perplexity.ai |
| OpenEvidence | openevidence.com |
| Calendar | system://calendar |
| Reminders | system://reminders |

Icons automatically display the website's favicon. You can also set a custom emoji in Settings.

**Tab Management**

- Clicking the same Destination icon opens a new page for that service
- Numbered dots appear below each Destination icon, representing open tabs
- Click a dot to switch tabs
- Press `⌘W` to close the current tab
- Hover over a dot to reveal the ✕ close button

**Adjusting Width**

Three buttons at the bottom of the sidebar (S / M / L):

| Mode | Width | Height |
|------|-------|--------|
| S | 1/3 screen | 50% |
| M | 1/2 screen | 70% |
| L | 2/3 screen | 85% |

### Keyboard Shortcuts

**Global Shortcuts (work in any app)**

| Shortcut | Action |
|----------|--------|
| `⌘⇧A` | Show/hide sidebar |
| `⌘⇧S` | Screenshot and send to AI |
| `⌘C` `⌘C` | Quick-send clipboard to a Destination |

**In-sidebar Shortcuts**

| Shortcut | Action |
|----------|--------|
| `⌘N` | Open new tab |
| `⌘R` | Reload current page |
| `⌘W` | Close current tab |

**Customizing Shortcuts**

1. Click the ⚙ Settings button at the bottom of the sidebar
2. In the "Keyboard Shortcuts" section, click the shortcut you want to change
3. The field will flash and show "Press keys..."
4. Press your desired key combination
5. The shortcut takes effect immediately and is saved automatically

### Advanced Features

**Quick Clipboard Send (⌘C ⌘C)**

1. Select the text you want to send
2. Press `⌘C` twice quickly (within 0.5s)
3. A Destination picker popup appears on screen
4. Click your target destination
5. Peekabrowser opens the page and pastes the text into the input field automatically

**Screenshot to AI (⌘⇧S)**

1. Press `⌘⇧S`
2. The sidebar hides automatically
3. Use the system screenshot tool to select a screen area
4. After capture, a Destination picker popup appears
5. Choose the AI service to send to
6. The screenshot is pasted into the input field automatically

> **Note:** First-time use requires granting permission in **System Settings → Privacy & Security → Screen Recording**.

**Multi-Monitor Support**

Peekabrowser supports multi-monitor setups. The sidebar appears on whichever screen your cursor is on.

**Calendar & Reminders Integration (v1.1.0)**

After adding Calendar and Reminders to your Destination list:

- **Click sidebar icon**: Opens the native macOS Calendar / Reminders app
- **⌘C ⌘C to send text**: Copy text then quick-press ⌘C twice, choose Calendar or Reminders, and a config window will pop up where you can:
  - Edit the title
  - Choose a calendar / reminder list
  - Set the date and time
- **Screenshot OCR**: Press ⌘⇧S to take a screenshot, then choose Calendar or Reminders. The system will automatically extract text from the image and fill the title field

**Reorder Destinations (v1.1.0)**

In the Settings page, each Destination has ↑↓ buttons to adjust the display order.

**New Tab & Open in Browser (v1.2.0)**

Two new buttons at the bottom of the sidebar:
- **+ (New Tab)**: Open a new tab for the current Destination, shortcut `⌘N`
- **↗ (Open in Browser)**: Open the current page in your default browser

### Settings

Click the ⚙ button at the bottom of the sidebar to open Settings.

**Managing Destinations**

- **Add:** Click "+ Add Destination", choose from presets or click "+ Custom" to enter a name and URL manually
- **Edit:** Click the ✎ button next to a Destination to modify its name, URL, or icon
- **Delete:** Click the ✕ button next to a Destination
- **Reorder:** Click the ↑↓ buttons to change the order

**Icon Settings**

- **Auto favicon:** Leave the Icon (emoji) field empty to automatically display the website's favicon
- **Custom emoji:** Enter any emoji in the Icon field to use it as the icon

### Data Storage

All settings are stored locally on your machine:

```
~/Library/Application Support/com.peekabrowser.app/
├── destinations.json    # Destination list
└── shortcuts.json       # Keyboard shortcut config
```

- Login state and cookies are preserved in the WebView
- Switching tabs does not log you out
- To fully uninstall, delete the folder above after removing the app

### Quitting

Right-click the Peekabrowser icon in the Menu Bar and select **Quit Peekabrowser**.

### Troubleshooting

**Can't open the app**
Go to **System Settings → Privacy & Security**, find the Peekabrowser prompt, and click "Open Anyway".

**Screenshot feature not working**
Go to **System Settings → Privacy & Security → Screen Recording** and make sure Peekabrowser is allowed.

**Google login fails**
Some third-party login flows may be restricted in embedded browsers due to Google's security policies.

**Sidebar not appearing**
Make sure Peekabrowser is running (icon in Menu Bar), then press `⌘⇧A` or move your cursor to the far left edge of the screen.

---

### 版本紀錄

**v1.4.0 — 多螢幕修正、Cmd+C+C 穩定性、Pin 固定功能**

- 🖥️ **多螢幕 auto-hide 修正**：側邊欄在副螢幕上現在能正確回應滑鼠移入移出的自動隱藏
- 📌 **Pin 固定功能**：側邊欄新增 📌 按鈕，開啟後 app 不會自動隱藏，僅能透過 `⌘⇧A` 手動隱藏
- 📋 **Cmd+C+C 穩定性提升**：修正複製檔案或圖片等非文字內容後，後續的 Cmd+C+C 功能會永久失效的問題
- 🔍 **搜尋框相容性擴充**：新增 `input[type="search"]` 支援，PubMed 等使用搜尋型輸入框的網站現在可正常自動貼上
- ⚡ **Hover detector 重構**：auto-hide 邏輯不再被螢幕邊緣偵測阻擋，確保所有場景下都能正常隱藏

**v1.3.0 — WebView 記憶體管理**

- Peekabrowser 採用 Window Pool Reuse 機制管理記憶體：關閉的分頁 WebView 會被回收並在開啟新分頁時重複利用，而非每次都建立新的 WebContent process。這讓長時間使用時記憶體消耗維持穩定，不會隨著開關分頁次數而持續增長。

**v1.2.0 — 新分頁按鈕、在瀏覽器中開啟**

**v1.1.0 — 行事曆/提醒事項整合、OCR、排序**

---

### Release Notes

**v1.4.0 — Multi-screen fix, Cmd+C+C reliability, Pin feature**

- 🖥️ **Multi-screen auto-hide fix**: Sidebar now correctly auto-hides on secondary monitors when the cursor enters and leaves the panel area
- 📌 **Pin feature**: New 📌 button on the sidebar prevents auto-hide; only `⌘⇧A` can hide the sidebar when pinned
- 📋 **Cmd+C+C reliability**: Fixed an issue where copying non-text content (files, images) would permanently break the Cmd+C+C detection until app restart
- 🔍 **Search input compatibility**: Added `input[type="search"]` support so sites like PubMed now auto-paste correctly
- ⚡ **Hover detector refactor**: Auto-hide logic is no longer blocked by edge detection, ensuring reliable hide behavior in all scenarios

**v1.3.0 — WebView Memory Management**

- Peekabrowser uses a Window Pool Reuse mechanism to manage memory: closed tab WebViews are recycled and reused when opening new tabs, instead of creating new WebContent processes each time. This keeps memory usage stable during long sessions, preventing growth from repeated tab open/close cycles.

**v1.2.0 — New Tab button, Open in Browser**

**v1.1.0 — Calendar/Reminders integration, OCR, Reorder**

---

Peekabrowser v1.4.0 | Built with [Tauri](https://tauri.app)
