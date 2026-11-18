# 競品與改名調查

調查日期：2026-11-18。這份文件支援 Joseph 在 production 完善工作之前決定產品定位與名稱。MyOwnKeys 目前是候選，尚未決定或執行改名。Resume Tailor 不列入本輪發布門檻。

## 結論

有直接競品，而且「集中管理 keys」「app 拿不到原始 key」「手機批准」都已有人做。沒有找到可靠證據，能證明某家已占據「個人跨 app、手機逐次審閱 AI 請求」的大部分市場；這也不代表需求已被證明。大型相鄰替代方案則很成熟。

MyOwnKeys 表意清楚，但不建議採用：App Store 已有同名的 My Own Keys: Home Move Planner，它的服務條款與隱私頁使用 myownkeys.com。這直接削弱這次改名想改善的搜尋辨識度。[App Store 官方頁面](https://apps.apple.com/us/app/my-own-keys-home-move-planner/id6762095980)

## 競品與實際威脅

公開數字是本次讀到的頁面快照，可能落後實際變化。公司自報規模、stars、商店使用人數與下載級距不能互換，更不能當作市佔率。

| 服務 | 與目前產品的重疊 | 公開規模訊號 | 我們目前落後或不宜正面競爭的部分 |
|---|---|---|---|
| Byoky | API key wallet、app session、授權撤銷、用量限制；有 iOS、Android、瀏覽器與 CLI | GitHub 顯示 17 stars；Chrome 商店 23 users；Firefox 3 users。這些數字不包含完整行動用戶 | 本機保管的信任模型、平台覆蓋、streaming、tool use、vision；功能已比本專案廣 |
| ShieldNode | 真實 keys 保管於其 vault，發給 agent 虛擬 key，手機推播允許／拒絕、緊急停止 | Google Play 顯示 10+ downloads；美國 App Store 尚無足夠評分顯示總覽。未找到完整付費用戶或市佔數據 | 手機批准已上架，且可保護一般 API；不能把手機批准宣傳成獨有功能 |
| OpenRouter | 使用者 OAuth 授權 app、由自己的 credits 付費、key 額度與期限 | 官方 2026-05-28 融資文章自報 8M+ developers、400+ models，該文後於 8 月更新；這不是今天的精確活躍用戶數 | provider／模型生態、既有整合與基礎設施投入。若開發者只需要使用者自行付推論費，直接串它可能已足夠 |
| LiteLLM、Portkey | gateway、virtual keys、預算與用量管理、路由及觀測 | 本次 GitHub 頁面分別顯示 63.5k、13.4k stars；不代表商業市佔 | 企業 gateway 的廣度與整合。我們不宜為了競品逐一補齊功能 |
| BrainLink | 使用者一次連結 AI 帳號／額度到不同 app、統一帳務 | 官網與 app 目錄可見；未找到可核實的大規模用戶數 | 「別再貼 API key」「使用者付費」已是既有產品主張，不是新類別 |

來源：[Byoky 官網](https://byoky.com/)、[開發文件](https://byoky.com/docs)、[GitHub](https://github.com/MichaelLod/byoky)、[Chrome 商店](https://chromewebstore.google.com/detail/byoky/igjohldpldlahcjmefdhlnbcpldlgmon)、[Firefox](https://addons.mozilla.org/en-US/firefox/addon/byoky/)、[ShieldNode 官網](https://shieldnode.app/)、[Google Play](https://play.google.com/store/apps/details?id=app.shieldnode.mobileandroid&hl=en)、[App Store](https://apps.apple.com/us/app/shieldnode-api-keys-for-ai/id6763352321)、[OpenRouter 規模公告](https://openrouter.ai/blog/announcements/series-b/)、[OAuth 文件](https://openrouter.ai/docs/guides/overview/auth/oauth)、[LiteLLM GitHub](https://github.com/BerriAI/litellm)、[virtual keys](https://docs.litellm.ai/docs/proxy/virtual_keys)、[Portkey GitHub](https://github.com/Portkey-AI/gateway)、[Portkey 官網](https://portkey.ai/)、[BrainLink 官網](https://www.brainlink.dev/users)。

### 兩個容易誤判的地方

Byoky 的舊商店文案還提到手機配對與 cloud vault，但目前官方文件明確寫每個 wallet 服務同一裝置，沒有手機與電腦配對；App Store 1.1.0 更新紀錄也列出移除 phone pairing、cloud sync 等功能。比較應以目前文件為準，不能把歷史功能都算成現在可用。[目前架構](https://byoky.com/docs)、[版本紀錄](https://apps.apple.com/us/app/byoky/id6760779919)

ShieldNode 官網強調的是批准一段 key 啟用時間，例如 5 分鐘或 8 小時；商店也有較廣泛的逐次敏感呼叫敘述。本輪未安裝或實測，不能據此斷言它一定沒有 payload review、逐次批准或費用審閱。[官方流程](https://shieldnode.app/)、[商店說明](https://apps.apple.com/us/app/shieldnode-api-keys-for-ai/id6763352321)

## 對產品方向的建議

我們目前最需要回答的，是使用者為什麼願意把 key 與請求內容交給 Joseph 營運的伺服器。第三方 app 拿不到 key，不代表營運者也拿不到；目前後端會解密 key 後代送，並保存 payload／response。這與 Byoky 的裝置本機保管有真實取捨，文案要說清楚。

值得驗證的定位是：「在 iPhone 看清工具要送出的內容與預估費用，再決定是否允許這一次使用自己的 AI 帳號。」逐次審閱、非前景通知、遠端 server app、可查歷史及撤銷，應形成一個陌生人能完成的流程。這是待驗證的差異，不是已成立的護城河。

建議維持原本的小規模試用目標：找至少兩個非本專案示例的開發者整合，觀察新使用者能否無口頭教學完成首次成功；另外問他們為什麼不用直接 OpenRouter OAuth 或 Byoky。也要觀察逐次批准是否造成疲勞。先用這些證據決定繼續投入的範圍。

## 名稱評估

| 名稱 | 判斷 |
|---|---|
| MasterKey | 語意泛用，未指向 AI；Joseph 已表明搜尋辨識度不佳 |
| MyOwnKeys | 容易理解、呼應 BYOK，但仍像一般 key manager，未表達批准；同名 App 與其 .com 已使用，建議排除 |
| KeyConsent | 可再評估的方向：讓名稱表達「授權／同意」。本輪精確字串搜尋未發現明顯同名軟體產品，但未完成域名註冊狀態、各地 App Store 名稱或商標查核 |
| Sorevan Keys | 可再評估的方向：沿用 repo 目前 bundle ID 中的 Sorevan 命名。本輪未發現明顯同名產品；是否採為對外品牌仍由 Joseph 決定，未完成可用性查核 |

MyOwnKeys 網域首頁未能由本輪瀏覽工具讀取；「已有使用」的依據是現有 App 的官方條款／隱私連結，並非宣稱已完成域名所有權或商標調查。其他候選也不能因為搜尋沒有明顯結果就視為可註冊。

## 全面改名的具體範圍

先選定唯一品牌拼法、repo slug、主網域及技術識別，再安排同一次切換。名稱尚未選定前，不要先更動其中一部分。

| 範圍 | 目前實際依賴 | 執行與驗證 |
|---|---|---|
| GitHub | iamjoseph331/MasterKey、origin、README badges／links、Pages | 改 repo 設定，更新 clones 的 remote、Render 的 repo 綁定及 CI；檢查 PR／branch protection／webhooks 與 Pages |
| 網站與搜尋 | /MasterKey/、導覽／教學／隱私／支援、social.svg、截圖、MK 圖示 | 更新頁名、metadata、canonical／分享圖與所有內外連結；若使用新網域，檢查 TLS 與 DNS。舊 Pages 路徑不會隨 repo rename 自動轉址 |
| 原生 App | ios/MasterKey/、Xcode target／scheme、App 名稱、三語資源、圖示、Keychain service | Codex 改自有路徑，保留並整合 Joseph 未提交的簽章設定；重建、安裝、原生 UI 檢查 |
| Apple 身分 | app.sorevan.masterkey、Apple audience、APNs topic、AASA 的 appID、provisioning | 查 App Store Connect 是否已上傳 build，再決定能否更換 bundle ID；如已上傳，更換 ID 需要新 app record。跨 Apple 登入、Keychain、APNs 與裝置資料的影響必須具體評估 |
| 連結與 OAuth | masterkey://connect、enroll、oauth/openrouter；Associated Domains；backend 產生的連結 | 原生解析與 Claude 的後端輸出同步改，驗證 Camera QR -> app、enrollment、真正 OpenRouter consent -> 回到 app；不以 mock 替代真機驗證 |
| 後端、SDK、examples | Cargo crate／binary、Docker CMD、demo scripts、HTML、MK_*、mk_*、X-MasterKey-Request-Id、masterkey_* error codes | 由 Claude 在自有路徑處理。環境變數、token／header／error 名稱若變動，必須同改 contract、clients 與測試，並在 AGENTS 記 CONTRACT CHANGE |
| 營運與資料 | Render service／URL、MK_PUBLIC_URL、SQLite 路徑、backup prefix、還原腳本、停轉送檔案位置 | 分別盤點 display name 與實際 URL。不得因換 service 或 DB 檔名就指向空資料庫；以既有資料驗證重啟、讀取、備份還原與刪除紀錄重播 |
| 文件與協作 | docs/PLAN.md、AGENTS.md、CLAUDE.md、操作指南、發布／審查材料 | 更新現行文件；歷史 log 依 append-only 規則保留，新增最終對照、commit 與切換結果。通知 Claude 更新它的 checkout remote／路徑與工作指令 |
| 本機 checkout | /Users/macbook/Code/Sideprojects/Masterkey，兩個 agent 可能仍在使用 | 等兩邊完成／停止使用舊 cwd，再搬資料夾並更新工作區。不要在另一個工作階段仍寫入時搬走 |

GitHub 官方明確排除 project site URL 的自動轉址：[repo rename 文件](https://docs.github.com/en/repositories/creating-and-managing-repositories/renaming-a-repository)。Apple 說明上傳 build 後不能改既有 App 的 bundle ID：[更換 bundle identifier](https://developer.apple.com/documentation/xcode/changing-the-bundle-identifier)。

Joseph 不要求 backwards compatibility，因此不預設保留雙套 SDK／scheme。但這不授權清空資料或重新產生加密主金鑰。程式中的通用密碼學術語 master_key 也不是品牌殘留；不能把它當作字串替換目標。既有加密值與任何 AAD 依賴都要在改名後仍可正確讀取。

## 本輪執行狀態與 Claude 交接

只做調查與遷移盤點，沒有改 GitHub 名稱、技術識別、付費資源或 production。工作目錄原有 iOS／site／簽章與語言資源的未提交修改，未納入本輪。

依協作規則嘗試 git pull --rebase，但被原有未提交修改阻止；沒有 stash、清掉或提交別人的工作。以上 repo 盤點針對目前 checkout；執行改名前需要重新同步。

Claude 的交接管道是 AGENTS.md。名稱選定後先記錄最終對照與分工，Codex 改 iOS／site／README／CI，Claude 改 backend／sdk／examples／Render；切換後再記錄實際 repo URL、連結與 API 差異、驗證結果及仍需 owner 操作的項目。沒有 Claude ACK 前，不把交接紀錄等同於已讀或已完成。
