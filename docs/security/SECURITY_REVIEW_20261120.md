# Keyala 安全性與加密現況分析 + 強化方案（2026-11-20）

> 範圍：`backend/`（Rust 伺服器、DB schema、加密、登入、配對、代理）、`ios/`（Keychain、Face ID）、`docs/ops/`（金鑰、備份）、Render 部署。
> 方法：**靜態閱讀原始碼與文件**，沒有做滲透測試、沒有對 staging/prod 發任何請求。嚴重度是我依「攻擊成本 × 損害」的判斷，不是實測結果。
> 對象：Joseph（決策）、Codex（iOS 相關項目）。

---

## 1. 一頁摘要

**結論：以個人實驗專案來說，「錢」這條線做得很紮實；「誰能動用金鑰」這條線仍然是單點信任。**

- 花費控制（預留額度、allowlist、at-most-once、首次只能 ask、撤銷與佇列的競態）都經過多輪 review，屬於這個產品最強的部分。
- 加密本身用法正確（AES-256-GCM、隨機 nonce、AAD 綁定 user|vendor），但**整個系統只有一把 master key**，放在 Render 的環境變數裡。能讀 Render dashboard 的人、或拿到「DB + 環境變數」的人，就能解開所有使用者的 API key。
- 認證上最大的缺口：**session token 永不過期、沒有綁定裝置、伺服器端沒有 step-up**。偷到一個 session token 就能把自己的 app 配對進來，接著把 grant 改成 `trust`，用受害者的 key 無上限地花費。Face ID 只在 App 端檢查，伺服器並不要求。
- 隱私面：**prompt 與回應以明文永久保存在 `requests` 表**，每日備份也是明文，而且備份和 DB 在同一顆碟上。

| 面向 | 強固度 | 一句話 |
|---|---|---|
| 花費/計費安全 | ●●●●○ | 多輪 review 過，fail-closed |
| 傳輸與上游 | ●●●●○ | 固定上游 URL、停用 redirect、rustls |
| 靜態加密（API key） | ●●●○○ | 演算法正確；單一金鑰、無版本、無輪替、無 KMS |
| 使用者認證/session | ●●○○○ | Apple 驗證正確；session 永久、無裝置綁定、無 step-up |
| 配對/新增 key 流程 | ●●●○○ | 設計用心；QR 先掃先贏、短碼可枚舉、origin 可偽造 |
| 資料最小化/隱私 | ●●○○○ | 推播已去敏；prompt/回應明文永久保存 |
| 營運/供應鏈 | ●●○○○ | 有備份與 runbook；無異地備份、金鑰託管未確認、CI 無 audit |
| iOS 端 | ●●●●○ | Keychain ThisDeviceOnly、ephemeral session、擋 redirect |

---

## 2. 系統與信任邊界

**要保護的資產（依價值排序）**
1. 使用者的供應商 API key（OpenAI / Anthropic / OpenRouter）：能直接花使用者的錢。
2. 動用 key 的權限：session token、grant token、App 的核准動作。
3. Prompt 與回應內容（隱私）。
4. `KEYALA_MASTER_KEY`：上面第 1 項的總鑰匙。
5. 開發者的 app secret 和報表。

**參與者與信任假設**

```
使用者 iPhone ──session──▶ Keyala 伺服器 (Render, SQLite on disk) ──使用者的 key──▶ OpenAI/Anthropic/OpenRouter
                                ▲
開發者伺服器/網頁 ──grant/app secret──┘
營運者 (Joseph) ── Render / GitHub / Cloudflare / Apple 帳號 ── 可讀 master key
```

- 架構本質上是**託管型（custodial）**：伺服器在轉發時必須拿到明文 key，因此營運者在技術上一定能讀到 key。`/privacy` 頁已誠實揭露這點，這是正確的做法。
- 主要威脅者：(a) 偷到 session 或 grant token 的人；(b) 釣魚網頁；(c) 拿到 DB 或備份的人；(d) 入侵營運者帳號（Render/GitHub）的人；(e) 惡意或被入侵的開發者 App。

---

## 3. 已經做對的地方（保留，別在重構時弄壞）

- **AES-256-GCM + AAD**（[lib.rs:453](../../backend/src/lib.rs:453)、`key_aad`）：密文綁定 `user|vendor`，無法把密文搬到別的列去解。enrollment 用 `enroll|id`，app secret 用 `app-secret`，彼此分開。
- **所有 token 只存 SHA-256 雜湊**（session、grant、app secret、connect secret、enroll secret、OAuth state）。token 是 256-bit 隨機值，因此單純雜湊就足夠。
- **Grant token 只交付一次**，遺失就重新配對。
- **Apple 登入**：固定 RS256（擋掉 `none`/HS256 攻擊）、檢查 iss/aud/exp、以 `sub` 當身分、從不用 email 合併帳號。JWKS 每分鐘最多重抓一次。
- **上游**：URL 固定、`redirect::Policy::none()`（避免 `x-api-key` 被轉送到別處）、rustls、admin key 拒收、新 key 用免費的 `/models` 驗證。
- **花費**：allowlist 參數、整數 micro-USD、原子預留、`sending → uncertain` 永不重試、Idempotency-Key、首次 grant 只能 ask。
- **帳號刪除**：先寫並 fsync tombstone 再 commit，失敗回 503；備份還原後可以 replay 刪除紀錄。
- **推播**：預設是通用文字，不洩漏 app/模型/金額。
- **iOS**：session 存在 Keychain `WhenUnlockedThisDeviceOnly`、`URLSession(.ephemeral)`、擋 redirect、核准一律開詳細畫面（鎖定畫面不能直接核准）。
- **錯誤訊息**：JSON 解析錯誤不回顯內容（避免把 key 印出來）。

---

## 4. 發現（依嚴重度）

### H1 — 單一 master key 是全系統的根，而且託管狀態未確認
- 一把 `KEYALA_MASTER_KEY`（或 fallback `MK_MASTER_KEY`）同時加密：所有 API key、enrollment 中的 key、app secret 的 sealed record（[lib.rs:520](../../backend/src/lib.rs:520)）。
- 金鑰放在 Render 的環境變數。**Render 帳號（或任何能看到 env 的協作者／被盜的 session）= 全部使用者的 key。** DB 在同一台機器上，所以兩者的距離只有一個 shell。
- 密文沒有 key id 或版本號 → 無法漸進輪替。RUNBOOK 寫明「輪替 = 所有人重新加 key」。
- `docs/ops/PRODUCTION.md:28`：master key 是否在密碼管理器裡**仍未驗證**。遺失 = 所有 key 都無法復原。
- app secret 的 seal 對外開放（`POST /v1/apps` 每 IP 每小時 10 次）：任何人都能讓伺服器用 master key 加密已知內容。GCM 在這種情況下仍然安全，但這代表同一把鑰匙同時用在不同信任等級的用途上，缺少用途分離（domain separation）。

### H2 — Session：永不過期、無裝置綁定、伺服器端沒有 step-up
- `auth_session`（[lib.rs:501](../../backend/src/lib.rs:501)）只查 `token_hash`，`sessions` 表沒有過期時間或最後使用時間。
- 一個 session 就能做所有高風險動作：換 key（`PUT /v1/me/keys`）、核准配對、**把 grant 放寬成 `trust`**（[routes.rs:941](../../backend/src/routes.rs:941) `patch_grant` 不要求任何再次驗證）、核准請求、刪除帳號以外的所有事情。
- 攻擊鏈：session 外洩（備份的 iPhone、越獄裝置、中間人、日誌、App bug）→ 攻擊者用自己的 app 建立 connect → 用偷來的 session 核准（首次只能 ask）→ 立刻 `PATCH` 成 `trust` → 用受害者的 key 不限量呼叫。`trust` 只有 `alert_usd_per_day` 會「提醒」，不會「擋」（[proxy.rs:62](../../backend/src/proxy.rs:62) 起）。
- 「核准前驗證 Face ID」（`requireDeviceAuthentication`）**預設關閉，而且只在 App 端檢查**。伺服器無法分辨這個核准是使用者本人操作，還是持有 token 的腳本送出的。

### H3 — 「從電腦新增 key」的 QR 先掃先贏
- 流程：電腦建立 enrollment → **任何**已登入的手機都能 claim（[enroll.rs:188](../../backend/src/enroll.rs:188)）→ 電腦上傳 key → 由那支手機確認。
- 電腦頁只看得到狀態變成 `claimed`，看不到「是哪個帳號 claim 的」（`add_key.html:112`）。
- 攻擊：在螢幕分享、直播、錄影，或被人從背後看到 QR 時，攻擊者搶先掃描 → 受害者照常貼上 key → **key 存進攻擊者的帳號**，攻擊者再透過自己的 grant 花用。這種攻擊和 WhatsApp Web 的 QR 劫持是同一類。

### M1 — 瀏覽器 App 的 origin 可以偽造，UI 卻把它呈現得像已驗證
- 公開 app 只用 `app_id` 加上 `Origin` header（[routes.rs:124](../../backend/src/routes.rs:124)）。非瀏覽器客戶端可以任意偽造 `Origin`，程式註解也寫明了這點。
- 攻擊：釣魚站的後端用 `Origin: https://keyala.io` 和 `app_resume_tailor` 取得 connect code 與 `connect_secret`，再把 code 放進仿冒頁面。使用者在手機上看到「Resume Tailor from keyala.io」，正好就是我們教使用者去核對的那個字串。
- 目前的緩解：首次只能 ask（每次呼叫仍要核准）。但只要使用者後來放寬成 trust，就會被利用。

### M2 — 短 connect code 可以枚舉，使用者端沒有限速
- `KY-XXXX` 只有 32⁴ ≈ 105 萬種組合，有效 10 分鐘。`GET /v1/me/connect/{code}` 和 `approve` 沒有任何限速，只需要一個（免費的）帳號。
- 攻擊：掃描所有 pending code，搶在真正的使用者之前**用攻擊者自己的 key 核准**。開發者的 App 收到的 grant 屬於攻擊者，之後那個 App 使用者的 prompt 會出現在**攻擊者的 Inbox**，造成隱私外洩。攻擊者也可以拿到 app 名稱與 origin，或把 code 全部 deny 掉（DoS）。
- 公開 app 建立 connect 的限速是每 IP 10 分鐘 20 次，而且存在記憶體裡、只對單一 instance 有效。

### M3 — Prompt 與回應明文永久保存
- `requests.payload`、`requests.response` 是 TEXT 明文，沒有保留期限（sweeper 只清 enrollment 與 OAuth）。
- 每日備份（同一顆碟、保留 7 份、`mode 600`）同樣是明文。任何 DB 外洩都會一次外洩所有對話內容。
- `/privacy` 寫的是「保留到刪除帳號」，與實作一致，但這是可以縮短的。
- **2026-11-20 已處理（dev 分支，尚未部署）：** migration 0013 加入每位使用者的資料金鑰（DEK），payload／response 以 `enc1:` 格式加密；內容在請求決定後 30 天刪除（`purged`），只留下費用、模型、token 數；開啟 `secure_delete`。剩下的限制：備份中的 DEK 也是用 master key 包裝的，所以刪除帳號之後，舊備份在輪替掉之前仍然可以解開（備份見 M6）。
- 附帶發現：之前沒有開 `secure_delete`，刪除帳號或換 key 之後，舊的密文與明文會殘留在 DB 檔的 free page 裡。測試 `deleted_account_leaves_no_content_in_the_database_file` 在關閉 `secure_delete` 時會失敗，開啟後通過。

### M4 — Grant token 是單純的 bearer，外洩後沒有第二道防線
- 開發者伺服器的日誌、`.env`、前端（瀏覽器 App 的 grant 存在頁面裡）任何一處洩漏 grant，持有者都能以那個 grant 的政策花錢。
- 沒有預設的到期時間、沒有綁定 app secret、沒有異常偵測（例如新的 IP 或國家、突然暴增的用量）。

### M5 — 網頁沒有安全 headers；SDK 沒有完整性保護
- `/add-key`（使用者會在這裡**貼上明文 key**）、`/connect/{code}`、`/enroll/{id}`、`/about` 都沒有 CSP、`frame-ancestors`/`X-Frame-Options`、HSTS、`Referrer-Policy`、`X-Content-Type-Options`。
- `/add-key` 使用 inline `<script>`。只要出現一個 XSS，key 就會在貼上的當下被偷走。
- `/sdk/keyala.js` 由伺服器動態提供，第三方網站直接引用。伺服器一旦被入侵，所有整合網站都會受影響；這個檔案沒有版本化路徑，也沒有 SRI。

### M6 — 備份與災難復原
- 備份和 DB 在同一顆碟上、沒有加密、沒有異地副本（文件已承認）。這只能防誤刪，防不了碟損或帳號被盜。
- 正向的一點：master key 不在 DB 裡，所以光拿到備份解不開 key。但 **prompt 不需要 master key 就能讀**（見 M3）。

### M7 — Apple identity token 沒有 nonce
- `apple::verify` 沒有檢查 `nonce`。一個外洩的 id_token 在有效期內（約 10 分鐘）可以重放，換到一個全新的永久 session（再接上 H2）。
- 帳號刪除的再驗證要求 `iat ≤ 5 分鐘`，這有幫助，但登入本身沒有這個限制。

### L — 低風險與衛生問題
- **限速在記憶體中**、重啟就歸零、信任 `X-Forwarded-For` 的最後一段。在 Render 後面這是對的；換到別的部署環境就會被偽造。
- **危險旗標沒有啟動防呆**：`KEYALA_VENDOR_MOCK=1` 或 `KEYALA_DEV_LOGIN=1` 如果誤設在公開的 https 主機上，伺服器會照常啟動。（dev login 只需要一個 email，不需要密碼。）
- **sealed app secret 的「DB 重置後自動復活」**：prod 已經是持久化的，這個機制只剩風險（還原舊備份後，被輪替掉的舊 secret 又能用了）。可以改成只在 free plan 上開啟。
- **密碼登入**用未加鹽的 SHA-256：因為密碼是隨機高熵，可以接受（程式已註明）。如果以後開放使用者自訂密碼，必須換成 argon2。
- **明文 key 在記憶體中**是一般的 `String`，沒有 zeroize。實務風險低。
- **CI 沒有** `cargo audit`/`cargo deny`、Dependabot，也沒有把 GitHub Actions pin 到 SHA。`main` 被 push 就自動部署到 prod，所以 GitHub 帳號 = prod。
- **App 註冊不需驗證**：任何人都能叫自己「OpenAI Official」。UI 已顯示「Unverified app」，這是對的。

---

## 5. 強化方案 brainstorm

依「效益 / 成本」分組。每一項標註大約的工作量（S ≤ 半天、M ≤ 數天、L = 一週以上 / 需要設計）。

### A. 金鑰管理（針對 H1）
1. **HKDF 分用途子金鑰**（S）：`master → HKDF("api-key-v1")`、`("enroll-v1")`、`("app-seal-v1")`。即使某個用途出問題，也不會連帶影響其他用途。
2. **密文加上 key id／版本前綴，並寫 rewrap 工具**（M）：`v1:` + nonce + ct。新增 `KEYALA_MASTER_KEY_NEXT`，背景把舊密文重新加密。輪替就不再需要「所有人重新加 key」。
3. **Envelope encryption：每位使用者一把 DEK**（M）：master key 只用來包 DEK。刪除帳號時刪掉 DEK，備份裡的密文就等於「加密抹除」（crypto-shredding），連帶解決備份不能立即清除的問題。
4. **外部 KMS**（M，約每月 $1 + 每次呼叫極少的費用）：DEK 由 AWS KMS／GCP KMS 包裝，伺服器只拿得到 KMS 憑證。好處是 (a) 光有 DB 加 env 不夠，還得能呼叫 KMS；(b) KMS 有稽核日誌，大量解密時可以發現並撤銷；(c) 能限制解密頻率。KMS 憑證仍然放在 Render env，所以這項的作用是「讓盜用可以偵測、可以撤銷」，不是讓盜用不可能。
5. **確認 master key 的託管**（S，Joseph）：放進密碼管理器，留一份離線紙本或硬體備份。寫下 break-glass 流程。

### B. 「手機持有解鎖材料」：讓伺服器平常解不開 key（研究級，L）
- 想法：每位使用者的 DEK 再被一把**手機 Secure Enclave 的 P-256 金鑰**包一層。伺服器平時只存雙重包裝的密文。
- **ask 模式**：手機在核准時用 Secure Enclave 解開外層，把這一次的解鎖材料隨核准一起送出。伺服器解密、轉發，用完立刻丟棄。這樣 **DB + master key 同時外洩也拿不到 ask-only 使用者的 key**，伺服器上也沒有任何能「自己替使用者花錢」的東西。
- **trust/budget 模式**需要伺服器在背景自己解密，所以使用者開啟這些模式時，要明確同意把一份伺服器可解的副本留在伺服器上（UI 寫成「允許背景使用」）。
- 代價：換手機、遺失手機時要重新加 key（或用 iCloud Keychain 同步的金鑰，安全性會降一級）；實作與測試都複雜。這是 Keyala 和一般 proxy 最有差異化的地方，值得當作 v2 的賣點來評估。

### C. 裝置綁定的核准（針對 H2，最高優先的結構性修正，M–L）
1. 登入時，App 在 Secure Enclave 產生 P-256 金鑰，透過 **App Attest** 證明它來自正版 App、未被竄改的裝置，然後向伺服器註冊公鑰。
2. **所有高風險動作都要用這把金鑰簽章**：核准請求（簽 `request_id + payload SHA-256 + 金額`）、核准配對、放寬 grant、換 key、確認 enrollment、刪除帳號。伺服器驗證簽章後才執行。
3. 簽章前可以要求 Face ID（建立金鑰時設定 `.userPresence` 存取控制），讓「需要本人」由**硬體強制**，不再只是 App 的一個開關。
4. 效果：偷到 session token 只能「讀」，不能核准、不能放寬權限。H2 的攻擊鏈因此斷掉。
5. 進一步可以做 DPoP 式的 session 綁定：每個 API 請求都附上簽章，被偷的 token 離開該裝置就無法使用。

### D. Session 衛生（S–M，可以先做）
- 加上 `last_used_at`、`expires_at`：閒置 30 天失效、最長 180 天（數字可討論）。
- `GET /v1/me/sessions` 列出裝置與建立時間，可以逐一登出。新裝置登入時推播通知其他裝置。
- 在 C 完成之前先做**伺服器端 step-up**：`PATCH grant → trust/budget`、`PUT key` 要求最近 5 分鐘內的 Apple id_token（沿用帳號刪除已有的機制）。

### E. 配對與新增 key（針對 H3、M1、M2）
1. **Enrollment 雙向確認碼**（S–M）：claim 後，電腦頁顯示「已被 `j***@icloud.com` 的手機掃描」，加上一組 4 位數比對碼；手機上顯示同一組數字，使用者確認一致後才出現貼上欄位。攻擊者搶先掃描時，受害者會看到不是自己的帳號。
2. **QR／連結改用長 token，短碼只留給手動輸入**（S）：QR 帶 128-bit 的 `KY-…` 長碼。手動輸入的 4 碼短碼對每個 session 每 10 分鐘最多試 10 次，對每個 IP 也設限速。
3. **使用者端限速**（S）：`/v1/me/connect/*`、`/v1/me/enroll/*/claim` 依 user 與 IP 限速，並記錄異常。
4. **瀏覽器 App 改用 OAuth 式的 redirect 交付 grant**（M）：grant token 只透過頂層 redirect 交付到註冊 origin 的 callback（`https://registered.origin/keyala/callback#grant=…`），或用 SDK 開一個 Keyala 自家 origin 的 popup，再以 `postMessage` 交給 **`event.origin` 吻合的**視窗。偽造 `Origin` 的伺服器因此收不到 token，origin 才真的變成「已驗證」。`connect_secret` 輪詢可以留給 server app 使用。
5. **UI 用語**（S，Codex）：在 E4 完成前，把 origin 標成「網站宣稱來自」，不要讓它看起來像已驗證。

### F. Grant 與花費的第二道防線（針對 M4）
- 新 grant 預設附帶到期時間（例如 90 天，可以延長）。`trust` 強制要設每日**上限**，不只是提醒。
- **異常偵測**（M）：grant 首次從新的國家或 ASN 呼叫、單日花費超過前 7 日平均的 N 倍時，自動降回 ask 並推播。
- **Server app 的 grant 綁定 app secret**（選用，S–M）：要求附上 `X-Keyala-App: <app secret>`。官方 SDK 可以用 `default_headers` 帶上，仍然是 drop-in。這樣只外洩 grant 不夠用。
- **鼓勵在供應商端限制損害範圍**（S，引導文案）：建議使用者為 Keyala 建立**專用的 project key 並設定支出上限**（OpenAI project budget、Anthropic workspace spend limit、OpenRouter key limit，OAuth 流程已經會預填）。這是最便宜、也最有效的保險，就算 Keyala 整個被攻破，損失也有上限。可以放進加 key 的流程裡。

### G. 資料最小化（針對 M3、M6）
- **保留期限**（S）：`payload`／`response` 在 N 天後清成 NULL（建議預設 30 天，使用者可以選「完成後立即刪除」）。計費需要的 metadata（模型、tokens、金額）留下。報表本來就只用 metadata。
- **以使用者 DEK 加密 payload／response**（M，和 A3 一起做）：DB 外洩時不再有明文對話，刪除帳號也能做到加密抹除。
- **加密的異地備份**（S–M）：用 `age` 以一把**和 master key 不同**的公鑰加密備份，上傳到 Cloudflare R2（有免費額度，可以開 object lock）。私鑰離線保存。

### H. Web 與 SDK（針對 M5）
- 對所有 HTML 回應加上：`Content-Security-Policy: default-src 'none'; script-src 'sha256-…'; style-src 'sha256-…'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; form-action 'none'; base-uri 'none'`，以及 `Strict-Transport-Security`、`Referrer-Policy: no-referrer`、`X-Content-Type-Options: nosniff`（S，一個 axum middleware 即可）。
- API 回應也加上 `Cache-Control: no-store`（key、session 相關的回應）。
- SDK 改用版本化路徑 `/sdk/keyala-0.2.0.js`（永不變更的內容），文件提供 SRI hash，舊路徑保留做相容（S）。

### I. 啟動防呆與設定（S）
- `public_url` 是 https 且不是 localhost 時，如果設了 `VENDOR_MOCK`／`DEV_LOGIN`，就拒絕啟動。
- 在持久化的 prod 上關掉 sealed app secret 的自動復活（`KEYALA_APP_SECRET_RESURRECT=0`）。

### J. 供應鏈與營運帳號（S，大多是設定，不用寫程式）
- 這是最可能被忽略、但實際風險最大的一塊：**Render、GitHub、Cloudflare、Apple Developer、密碼管理器全部開啟硬體金鑰或 passkey 的 2FA**。這幾個帳號任何一個被盜，就等於 H1。
- `main` 設定 branch protection（AGENTS.md 已經寫了「will be」）：要求 PR、要求 CI。對 `render.yaml` 與 `.github/` 加 CODEOWNERS。
- CI 加入 `cargo audit` 與 `cargo deny`（授權與漏洞檢查）、Dependabot，並把 Actions pin 到 commit SHA。
- 發布 `/.well-known/security.txt`（`security@keyala.io`），並寫一份簡短的負責任揭露政策。

### K. 驗證與測試
- `cargo-fuzz`：pricing allowlist 解析器、reservation 計算、`origin_ok`（M）。
- 為 H2、H3、M2 寫回歸測試（攻擊者的 session 不能放寬政策、搶先 claim 時看得到差異、枚舉會被限速）。
- 正式公開或上 App Store 之前，找外部做一次小型滲透測試（L，需要預算）。

### L. 長期 / 研究
- **機密運算**：把解密與轉發放進 AWS Nitro Enclave 或 GCP Confidential VM，搭配 reproducible build 與遠端證明，在 App 裡顯示「目前執行的是第 X 版開源程式碼」。這是讓營運者本人也讀不到 key 的路線，和 B 可以互補，但成本與複雜度都高。
- **透明度日誌**：每次解密 key 都寫入一份使用者可在 App 查看的 append-only 紀錄（「你的 key 在什麼時間因為哪個請求被使用」）。

---

## 6. 建議路線圖

| 階段 | 內容 | 擁有者 | 規模 |
|---|---|---|---|
| **P0 本週（低成本、高效益）** | J 帳號 2FA 與 branch protection；A5 master key 託管；H 安全 headers；I 啟動防呆；E2/E3 長碼與限速；D session 過期 + step-up 放寬 grant／換 key；M7 Apple nonce（App 送 nonce，伺服器驗證）；CI `cargo audit` | Joseph／Claude／Codex（nonce 的 App 端） | 各 S |
| **P1 下個迭代** | E1 enrollment 確認碼；G 保留期限 + 加密異地備份；A1 + A2 HKDF 與 key 版本 + rewrap；F 到期預設、trust 硬上限、供應商端限額引導；E5 origin 用語 | Claude（backend）＋ Codex（UI） | S–M |
| **P2 結構性** | C 裝置綁定核准（Secure Enclave + App Attest）；E4 grant 的 redirect／postMessage 交付；A3 + G 每使用者 DEK 與 payload 加密；F 異常偵測 | 兩邊合作，需要 CONTRACT CHANGE | M–L |
| **P3 差異化／研究** | B 手機持有解鎖材料；A4 KMS；L 機密運算、透明度日誌；外部滲透測試 | 待 Joseph 決定方向 | L |

**如果只能做三件事：** (1) 營運帳號 2FA + master key 託管；(2) session 過期 + 放寬權限時 step-up；(3) prompt 保留期限。三件加起來大約一天的工作量，能擋掉最可能發生的幾種事故。

---

## 7. 需要 Joseph 決定的問題

1. Prompt／回應預設保留幾天？要不要提供「完成後立即刪除」？（會影響 App 的歷史紀錄功能）
2. 是否接受「trust 模式必須設定每日上限」這個 UX 限制？
3. B（手機持有解鎖材料）要不要列為 v2 的產品方向？它會改變「換手機」的體驗。
4. 是否願意為 KMS（每月約 $1–2）或外部滲透測試付費？

## 8. 這份分析沒有涵蓋的部分

- 沒有實際攻擊 staging／prod，也沒有檢查 Render、Cloudflare、GitHub 的帳號設定（2FA、協作者、token）。
- 沒有逐行審查 `pricing.rs` 的 allowlist 與 `ops.rs` 的刪除 cascade（之前已經過 Codex 與測試多輪 review）。
- 沒有審查 iOS 的 `Views.swift` UI 邏輯細節、網站（`site/`）的 JS，也沒有審查第三方依賴的漏洞（等 `cargo audit` 跑出結果）。
- 嚴重度是推論，不是實測結果。
