# Peekabrowser iPad 版移植筆記（2026-11-19）

來源 repo：`~/claude_domain/vibe-coding/iOS-Peekabrowser`（main，唯讀調查）。路徑皆相對於該 repo，除非標明 worktree。

## 1. 「Ask elsewhere」/ 到別處問 按鈕（change `lookup-loop`）

### 決策來源
- `openspec/decisions/INDEX.md:265`（passive 工具列按鈕、無 banner、INDEX 129 不變、同 query 切換、Records "Ask again"、不做多目的地同送）。
- `INDEX.md:351-357`：按鈕留在「由 query 開啟的分頁」工具列 + 快捷鍵；把該分頁**原始 query** 送到另一個目的地的**新分頁**（INDEX 224：每次 query 開新分頁、20 分頁上限適用）；選單沿用 SearchElsewhere 的選項；已回答過同 query 的目的地做標記，點它是**切換到該分頁**而非重送；快捷鍵循環同 query 分頁；Records 列加 "Ask again" 與 "Ask elsewhere"。
- 8 秒 capsule（`SearchElsewhereState.lifetime = 8`，`App/Sources/Logic/SearchElsewhere.swift:7`）即將移除，改成按鈕上一次性可關閉的 tip；macOS 只需要按鈕，不必移植 capsule。

### 點按後用哪段文字
- 一律用**該分頁的原始 query**（Records 的 `SearchRecord.text`），不是頁面選取、也不是網址列：`App/Sources/UI/AppModel+LookupLoop.swift:7-9`（`originalQuery(of:)`）。
- 分頁 → query 的對應：`WebTab.historyQueryID = lastRecordID ?? originQueryID`（`App/Sources/Web/WebTab.swift:37`）；還原的分頁靠 `RestoredQueryOrigin.find`（`App/Sources/UI/AppModel+Library.swift:41-51`、`App/Sources/Logic/LookupLoop.swift:36-41`）。
- 選單標題顯示 query（最多 2 行）：`App/Sources/UI/AskElsewhereViews.swift:51-57`。

### 目的地清單怎麼建、怎麼排序
- `LookupLoop.menu(...)`：`App/Sources/Logic/LookupLoop.swift:71-80`，底層用 `SearchElsewhereState.choices`（`App/Sources/Logic/SearchElsewhere.swift:23-26`）。
- 規則：`destinations.filter { $0.id != sentToID }`（**排除該 query 當初送去的目的地**，不是排除目前所在分頁的目的地）→ 其他目的地依使用者清單順序；後面接 prompt presets（`QueryLens`，`.lens`），來源 `settings.prompts(for: query.destinationID)`（`AppModel+LookupLoop.swift:24-28`）。
- 目的地範圍 = `destinations.searchDestinations`（排除釘選頁面 pageDestinations，`App/Sources/Store/DestinationStore.swift:68-70`）。
- 已回答標記：`sameQueryGroup`（開啟中且原始 query 文字完全相同的分頁，`LookupLoop.swift:53-56`）→ `answeredDestinations`（優先選不是目前分頁的那個，`:60-65`）→ MenuItem.switchTo。prompt 列永不標記。UI 顯示綠色 "Answered" checkmark（`AskElsewhereViews.swift:80-86`）。

### 選擇後的行為（新分頁 or 取代）
- `LookupLoop.action`（`LookupLoop.swift:84-90`）：已標記 → `.switchTo(tabID)`（`AppModel+LookupLoop.swift:44,55-61` 選取該分頁，不新增 Record）；未標記目的地 → `.send(text: query.text, destinationID, lensPrompt: "")`；prompt 列 → 送回**原目的地**並帶 `lens.prompt`。
- 送出走一般 `send(...)`（`App/Sources/UI/AppModel.swift:1724`）→ `fill` → `injectionTab`（`AppModel.swift:1866-1879`）：每次 query 開**新分頁**，只有「未動過的首頁分頁」才重用；超過 20 分頁時 `queryTab`（`:1883-`）先關最久沒看的可關分頁。**不會取代目前頁面。**
- 送出前仍受 consent、sign-in 檢查（`AppModel.swift:1734-1739`、`fill` `:1905-`）。

### 啟用／隱藏條件
- 按鈕只在目前分頁的原始 query 解析得到時顯示：`showsAskElsewhereButton`（`AppModel+LookupLoop.swift:37`）。沒由 query 開啟的分頁（例如純 Web 瀏覽分頁）→ 按鈕不出現、⌘E 變灰（`App/Sources/Logic/BrowserCommands.swift:163`，`ctx.hasOriginalQuery` 由 `AppModel+Browser.swift:165` 給）。Translate 發起的搜尋也有按鈕（無 banner）。

### UI 與符號
- SF Symbol：`arrow.triangle.branch`（`AskElsewhereViews.swift:13`；Records 動作 `:121`；⋯ 選單 `DestinationRail.swift:597`）。44x44 無邊框按鈕，a11y label "Ask elsewhere"，id `askElsewhereButton`（`AskElsewhereViews.swift:5-22`）。
- 位置：工具列導覽群組內，順序 `openInDefaultBrowser, askElsewhere, newTab`（`App/Sources/UI/AddressCapsuleView.swift:272-277`、`TopChrome.swift:36`）；窄寬度收進 ⋯ 選單（`DestinationRail.swift:591-598`）。
- 彈出：popover 綁 `model.askElsewhereOpen`（`AskElsewhereViews.swift:26-41`；狀態 `AppModel.swift:264-265`）；列表元件 `AskElsewhereMenuList`（`:45-100`，每列 44pt，圖示 24pt + 名稱 + Answered）。

### 快捷鍵
- ⌘E「Ask Elsewhere」（`BrowserCommands.swift:98`）→ `openAskElsewhere()` 開 popover（`AppModel+LookupLoop.swift:64-68`；dispatch `AppModel+Browser.swift:220`）。
- ⌥⌘]「Next Tab with Same Query」（`BrowserCommands.swift:99`；啟用條件 `:164`）→ `cycleSameQueryTab()` 在同 query 分頁間循環（`AppModel+LookupLoop.swift:72-83`，`LookupLoop.next` `:96-99` 會 wrap，只有一個分頁時回 nil）。

### Records
- 列的 swipe／context menu／popover：`App/Sources/UI/RecordsView.swift:238-266`；標題與 symbol 在 `AskElsewhereViews.swift:108-124`（Ask again = `arrow.clockwise`）。
- "Ask again" = `askAgain(record)` → `reopen`（`AppModel+LookupLoop.swift:52`、`AppModel.swift:2146-2149`）：同文字、同目的地、新分頁。"Ask elsewhere" 用同一個 `AskElsewhereMenuList`，但 `askElsewhereMenu(for: record)` 沒帶 `currentTabID`。
- 頁面歷史（LibraryViews）另有一個 "Ask again" 按鈕：`App/Sources/UI/LibraryViews.swift:69`。

### 測試與相關檔
- 純邏輯 `App/Sources/Logic/LookupLoop.swift`；接線 `App/Sources/UI/AppModel+LookupLoop.swift`；UI `App/Sources/UI/AskElsewhereViews.swift`；測試 `Tests/PeekabrowserTests/LookupLoopTests.swift`；字串 `App/Resources/Localizable.xcstrings:704,714`。
- macOS 移植建議：邏輯層（original query → menu → send/switch → cycle）幾乎可照搬；差異只在 UI 容器與 20 分頁上限的 macOS 對應。

## 2. 「Add Site」目錄 sheet

### 2.1 DestinationCatalog（`App/Sources/Logic/DestinationCatalog.swift`，main 195 行）
- 分組列舉 `CatalogGroup`（`:4-20`），顯示順序 = `allCases`：`general` "General search" → `ai` "AI chat" → `academic` "Academic & medical" → `library` "Library" → `reference` "Reference & translation" → `daily` "Everyday"。
- 欄位 `CatalogEntry`（`:25-40`）：`id`、`name`、`group`、`homeURL`、`submit`（`.url` / `.inject`）、`urlTemplate`（含 `{q}`）、`providerRule`（providers.js 規則 id）、`needsLogin`、`notVerified`（URL 格式未驗證）、`builtInID`（對應內建目的地）。
- 三個工廠：`.url`（`:42-47`，home 預設取 template 的 host）、`.inject`（`:49-53`）、`.builtIn`（`:55-59`）。轉成使用者目的地 `makeDestination()`（`:64-68`）；是否已加入 `isAdded`（`:71-73`）。
- 條目表 `DestinationCatalog.entries`（`:83-174`）：general 85-94、ai 96-111、academic 113-141、library 143-144、reference 146-166、daily 168-173。`own-library` 是特殊列（`:78`，開自訂圖書館表單，不是條目）。
- 「Removed list」：不在程式碼裡，而是文件 `research/42-catalog-site-check.md:84-89`（`## Removed`）列出被移除的 Epocrates（HTTP 403）、Reddit（bot-wall）；移除時使用者已加入的不動（destinations.json 不改）。改動歷史見 `:91-100` `## Fixed`。
- 語系預設目的地由目錄組成：`referenceDefaults`（`DestinationCatalog.swift:190-194`）。

### 2.2 providers.js / providers-ipad.js
- `App/Resources/providers.js`（455 行，源自 macOS `src-tauri/assets/providers.js` e6a1c0f）：
  - 規則表 `PROVIDERS` 從 `:39` 起，每筆 `{id, match(hostname), supportsText, input[], send[], stop[], response[], knowsCompletion, submitForm?}`；inject 型目錄站在 `:97-195`；找不到 host 時用 `GENERIC`（`:194-200`）。
  - 找輸入框：`first(selectors)` 依序試每個 selector，取**第一個可見**元素（`:16-35`）；`provider()` 以 `location.hostname` 比對（`:201-206`）。
  - 填值 `setText`（`:209-224`）：`textarea/input` 用 prototype 的原生 `value` setter + 派發 `input`/`change`（繞過 React 受控元件）；contenteditable 用 `execCommand('selectAll')` + `insertText`，失敗退 `textContent` + `InputEvent`。
  - 送出 `submit`（`:235-247`）：`submitForm`（Google）直接 `form.submit()`；否則每 250 ms 輪詢 send 按鈕最多 8 次（按鈕非 disabled 才 click），仍沒有就對輸入框發 Enter keydown/keypress/keyup。
  - 入口 `P.inject`（`:382-404`）：重複 requestId 回 `duplicate`；`readyState === 'loading'` 回 `not_ready`；找不到輸入框回 `no_input`；image 走 paste；文字成功回 `ok`。`P.status`（`:371-379`）回報 `ready/hasInput/generating/draft`。
  - 註解 `:97-98`：沒有 send 按鈕的站會在 retry 後按 Enter。
- `App/Resources/providers-ipad.js`（191 行）：只用 `P` 的公開入口，iPad 專屬功能：`signInState`（`:49-62`，回報證據不下結論）、`glowInput`（`:64`）、`clearInput`（`:82`）、`submitDraft`（`:104-115`，Send capsule 用）、`pageInput`（`:127-`，idle 卸載前判斷有沒有使用者打的字）。注意 `INPUT` 表（`:6-14`）只有內建 6 站 + generic，要與 providers.js 同步。
- 等待與重試在 Swift：`SendFlow.send`（`App/Sources/Web/Injection.swift:198-`）；`InjectionRetrySchedule`（`:4-27`，起始 0.3 s、每次 +0.2 s、上限 1.0 s、總等待上限 25 s）；`call`（`:296-317`）每次嘗試前先 sleep，檢查 `abortIf`（signed out → "aborted"）與 `onSite`（頁面離開該網站 → `left_site`），RPC 失敗當 `not_ready`；只有 `ok/duplicate/unsupported` 結束迴圈。`url` 型直接載入 `SearchTemplate.searchURL`（`App/Sources/Logic/SearchTemplate.swift:65-68`），載入成功才算 sent（`Injection.swift:204-217`）。
- 失敗後備援：`AppModel.fill`（`App/Sources/UI/AppModel.swift:1905-`）失敗時把 prompt **複製到剪貼簿**並顯示失敗原因（`FailureCause.classify`，離線 / 人機驗證有重試）；成功送出 2 秒後重讀 URL 存進 Record（`Injection.swift:176,` `refreshURL` `:319-`）。

### 2.3 登入／登出判斷（signed-in vs signed-out）
- 證據收集在頁面內：`P.signInState`（`providers-ipad.js:49-62`）回 `{hasInput, loaded, signedOutMarker, signedInMarker}`；標記表由 App 傳入。
- App 端表與判斷：`App/Sources/Logic/SignIn.swift`：`SignInHeuristics.table`（`:31-70`，每站 `signedIn` / `signedOut` selector、`signedOutTexts`、`inputWithoutSignInCounts`，目前只有 ChatGPT/Claude/Gemini/Perplexity/OpenEvidence）、`decide`（`:78`）、`SignInDetector.isSignInURL`（`:107`，登入頁 URL 即視為 signed out）、`state(for:url:probe:hasSessionCookie:)`（`:141-157`）、`SignInCookies.hasSession`（`:180`，cookie 當 signed-in 的正面證據）。自訂／目錄新增站屬 `.generic` → 永遠 `unknown`（輸入框不算登入證據，`:137-140`）。
- 觸發：`AppModel.pageEvent`（`AppModel.swift:2200-2236`）每次 `.loaded` 與 `.urlChanged`（SPA 等 1.2 s）呼叫 `probeSignIn`（`:2253-2280`）：最多 2 次、間隔 2 s，同時讀 `WKWebsiteDataStore` cookie；Google 只看 cookie 且搜尋不需登入（`:2257-2262`）。
- 「Sign-in wait」：`SignInGate`（`SignIn.swift:205-228`）：已知 signed-out 才等，unknown 仍嘗試填；`waitForSignIn`（`AppModel.swift:1834-1838`）存下待送 query 並顯示 needsSignIn；狀態變 signed-in 時 `stateChanged` 回傳待送 query 再 `fill`。`fill` 在送出失敗且當下 signed-out 時才進入等待（`AppModel.swift:1918-1935`）。`needsLogin` 旗標本身只是目錄標籤（`CatalogEntry.needsLogin`），不參與判斷。
- 登出／換帳號：`SiteData`（`SignIn.swift:253-`）、`AppModel.signOut`（`:2290-`）。

### 2.4 Add Site sheet 與進行中的 change `add-site-sheet`
- 分支 `opsx/add-site-sheet`，worktree `~/claude_domain/vibe-coding/.opsx-worktrees/iOS-Peekabrowser--add-site-sheet`（HEAD `ec9a7ab`）。main 尚未合併。
- 在 worktree 已有的實作（tasks.md 標 [x]，見 `openspec/changes/add-site-sheet/tasks.md:5-22`）：
  - 「UpToDate Expert AI」：`App/Sources/Logic/DestinationCatalog.swift:112-114`（id `uptodate-expert-ai`、`.ai` 組、home `https://ai.uptodate.com/`、`login: true`、inject 規則）；providers.js 規則 `App/Resources/providers.js:150-158`（`match: h === 'ai.uptodate.com'`，input 先找 placeholder 含 Ask/question 的 textarea，再退 contenteditable；send 找 `data-testid="send-button"` 與 Send/Submit/Ask/傳送/送出）。既有 `uptodate` URL 條目不變。
  - 搜尋：`App/Sources/Logic/CatalogSearch.swift`（比對名稱 + URL template/homeURL/host，不分大小寫，跨所有組，無相符的組隱藏）。
  - UI：`App/Sources/UI/CatalogView.swift:3,73,98`（標題 "Add Site"／「新增網站」、頂端搜尋欄；無相符顯示 "No matching sites" 與 Custom URL 按鈕）；rail 的「+」改開同一個 sheet（`App/Sources/UI/DestinationRail.swift:56`）。
  - 測試 `Tests/PeekabrowserTests/AddSiteSheetTests.swift`；`Try it` 只在真的送出後打勾的修正（commit `584dadf`）。
  - UpToDate Expert AI 的簽入後實送仍待使用者真機確認（`research/42` 的 Device check 段會列為 awaiting）。
- main 的狀態：以上全部「進行中／未合併」；main 的 `DestinationCatalog.swift` 沒有 uptodate-expert-ai，行號以 2.1 為準。

### 2.5 逐站測試結果
- `research/42-catalog-site-check.md`（155 行，由 `scripts/test.sh sites` 產生，勿手改）：結果表 `:9-83`（70 站：pass 44、fail 1、needs-login 18、bot-wall 7；模擬器、未登入、test query `aspirin`、每站 30 s）；`## Removed` `:84`、`## Fixed` `:91`、`## Needs login / bot wall` `:102`、`## Device check` `:131`、`## Passed on the URL only` `:146`、`## Candidates` `:152`。
- 原始站點研究：`research/20-search-site-catalog.md`。
