# Keyala 開發者文件

Keyala 讓你的應用使用個人自己的 OpenAI、Anthropic 或 OpenRouter API 金鑰，卻不會收到該金鑰。使用者配對一次後，應用取得限定範圍的授權權杖；使用者在手機批准呼叫，或設定用量上限。保留官方 SDK，只需修改 base URL。這份 Markdown 包含完整開發文件，也可直接提供給 AI 程式助理作為整合參考。

代管服務：正式環境 `https://app.keyala.io`，測試環境 `https://staging.keyala.io`。既有核心 API 可用於兩者；這次新增的 Coupon API 僅於 staging 驗證。註冊、配對、呼叫與報表務必使用同一伺服器。Keyala 是免費的實驗性 beta，限制請見最後一節。

## 贊助 Coupon（staging 驗證）

Coupon 正在 staging 驗證，這不代表正式環境已開放。手機、Console 與 SDK 必須使用同一伺服器。

登入 Console 後選擇 App，設定一批 Coupon 的**專用服務商 Key**、單一服務商、允許模型、每張額度、整批預算與領券截止日。每張效期從領券時計算，預設七天。開發者直接向服務商付費，Keyala 不販售額度，也不經手資金。贊助 Key 加密儲存在伺服器，不會回傳給領券者或連接的 App。請使用獨立設限的 Key，確認服務商條款與帳務控制；Keyala 的額度限制不是服務商帳單的保證。

分享批次的 `/coupon/KC-…` 連結或 QR。使用者登入後即可預覽與領券，不必新增個人 Key。每帳號每批只能領一次，到期也不能重新領取。領券時才從整批預算保留完整單張額度，分享連結不占用預算。剩餘可分配預算不足一張時停止新領券，已領券者保留自己的額度。到期或撤銷只回補尚未使用的部分一次；執行中及結果不明的預留繼續保留。

在 `POST /v1/connect` 傳入 `coupon_code`，或使用網頁 SDK 的 Coupon 選項，明確選擇贊助來源。Coupon 必須屬於正在連接的 App 與服務商。領券本身不等於授權 App。使用者仍需批准連接與**每一次呼叫**，可查看完整 prompt 與已記錄回應、付款方、剩餘額度及效期。贊助授權不能擴大為次數、預算或信任的自動批准；伺服器強制檢查模型清單。服務商請求格式與輪詢方式不變。

Coupon 到期、撤銷或不足以涵蓋請求預估費用時，回傳明確的 Coupon 錯誤，**不會自動改用個人 Key**。若要自行付費，必須另行明確建立 BYOK 連接。

批次報表只提供領取數、首次成功人數與請求／模型支出彙總，不揭露領券者身份、prompt 或回應。服務商回報實際費用、估算及結果不明預留分開呈現。額度通知讓發行者知道無法再分配完整單張額度或整批支出額度耗盡；通知不負責限制支出。伺服器營運者未設定服務時 Email 不啟用。本次以 mock 模型與通知驗證，尚非付費服務商呼叫或真實收件證明。

| 方法 | 路徑 | 用途 |
|---|---|---|
| GET / POST | `/v1/me/apps/{app_id}/coupons` | 建立／列出批次 |
| DELETE | `/v1/me/apps/{app_id}/coupons/{batch_id}` | 撤銷批次 |
| GET | `/v1/me/apps/{app_id}/coupons/{batch_id}/report` | 彙總報表 |
| GET / POST | `/v1/me/coupons/{code} / /v1/me/coupons/{code}/claim` | 預覽／領券 |
| GET | `/v1/me/coupons` | 列出自己的 Coupon |

發行端路由要求已登入的 App 擁有者，領券路由要求使用者登入。建立批次的內容為：

```json
{"vendor":"openai","models":["gpt-4o-mini"],"api_key":"REPLACE_WITH_DEDICATED_PROVIDER_KEY","per_coupon_usd":1,"total_usd":20,"validity_days":7,"claim_deadline":"2026-12-31T23:59:59.000Z"}
```

```js
const { grant_token } = await Keyala.connect({
  app: "app_your_app", vendor: "openai", coupon: "KC-REPLACE_WITH_BATCH_CODE"
});
```

## 1. Keyala 與呼叫流程

你的應用要求 Keyala 連結使用者。使用者在 Keyala iOS App 批准後，應用取得綁定該使用者、應用與單一服務商的 `grant_token`。應用以此權杖送出一般的服務商請求。Keyala 檢查政策、預留預估費用，必要時要求手機批准，再以使用者儲存的金鑰轉送請求，並原樣回傳服務商的回應。

```
your app --grant token--> Keyala server --(ask user if policy says so)--> user's phone
                              |  approved                                  approve / deny
                              v
                    provider (OpenAI / Anthropic / OpenRouter), user's key
                              |
your app <---- provider response, unchanged ---+
```

## 2. 快速開始（5 分鐘）

### 2.1 註冊應用

```sh
curl https://app.keyala.io/v1/apps \
  -H 'Content-Type: application/json' \
  -d '{"name":"My AI app","homepage":"https://yourapp.example"}'
```

```json
{ "app_id": "app_...", "app_secret": "keyala_app_..." }
```

`app_secret` 只顯示一次。請存放於伺服器端的秘密管理工具，勿放入瀏覽器程式、repo、網址或 log。每個來源位址每小時最多註冊 10 次，超過回傳 `429 rate_limited`。`name` 必填，最多 100 字元。

純瀏覽器應用也需送出 `allowed_origins`：1 至 10 個完整 origin，例如 `["https://yourapp.example","http://localhost:3000"]`，包含協定與主機、不含路徑；`http://` 僅限 localhost。`app_id` 可公開放在網頁程式中，secret 則不可。輪替或遺失 secret 都不會洩漏使用者金鑰。

### 2.2 連結使用者

依程式執行的位置選擇流程。

**伺服器應用。** 使用 app secret：

```sh
curl https://app.keyala.io/v1/connect \
  -H "Authorization: Bearer $KEYALA_APP_SECRET" \
  -H 'Content-Type: application/json' \
  -d '{"vendor":"openai"}'
```

```json
{
  "connect_code": "KY-4F7Q",
  "connect_url": "https://app.keyala.io/connect/KY-4F7Q",
  "connect_qr_url": "https://app.keyala.io/connect/KY-4F7Q/qr.svg",
  "expires_at": "2026-11-20T12:10:00.000Z"
}
```

`vendor` 為 `openai`、`anthropic` 或 `openrouter`。顯示 QR code（`connect_qr_url` 是 Keyala 提供的 SVG），或顯示連結與配對碼。配對碼有效 10 分鐘。以同一 secret 輪詢：

```sh
curl https://app.keyala.io/v1/connect/KY-4F7Q -H "Authorization: Bearer $KEYALA_APP_SECRET"
```

`status` 為 `pending`、`approved`、`denied` 或 `expired`。等待中另有 `seen` 提示：`null`、`"web"`（已在瀏覽器開啟連結，可能尚未安裝 App）或 `"app"`（手機正在檢視）。批准後，`{"status":"approved","grant_token":"keyala_grant_..."}` **只回傳一次**；再次輪詢前先儲存權杖。若遺失，請重新配對。

**瀏覽器應用。** 註冊時設定 `allowed_origins`，再使用同一伺服器提供的網頁 SDK。它會開啟對話框：電腦顯示 QR code 與配對碼，手機顯示「開啟 Keyala」按鈕；成功後回傳授權：

```html
<script src="https://app.keyala.io/sdk/keyala.js"></script>
<script>
  let grant;
  async function ensureGrant() {
    grant ??= (await Keyala.connect({ app: "app_...", vendor: "openai" })).grant_token;
    return grant;
  }
</script>
```

`Keyala.connect({ app, vendor, lang? })` 失敗時的 `err.code` 為 `cancelled`、`denied`、`unreachable` 或 `failed`。也可使用 `<keyala-connect app="app_..." vendor="openai"></keyala-connect>` 按鈕；成功時觸發 `connected` 事件，權杖位於 `detail.grant_token`。SDK 不儲存權杖；除非已設計適當的儲存方式，否則請放在記憶體中。不用 SDK 時，從允許的 `Origin` 發送 `POST /v1/connect`，內容為 `{"app_id":"app_...","vendor":"openai"}`，不帶 Authorization header；未允許的來源回傳 `403 origin_not_allowed`，每個位址每 10 分鐘最多 20 個配對碼。回應另含 `connect_secret`（`keyala_cs_...`），用來授權輪詢 `GET /v1/connect/{code}`。

### 2.3 發送呼叫

以 grant token 作為 API key，Keyala 伺服器作為 base URL。停用 SDK 自動重試，詳見 2.4。

```python
from openai import OpenAI

client = OpenAI(base_url="https://app.keyala.io/v1/openai", api_key=GRANT_TOKEN, max_retries=0)
reply = client.chat.completions.create(
    model="gpt-4o-mini",
    max_tokens=256,
    messages=[{"role": "user", "content": "Hello!"}],
)
```

```python
from anthropic import Anthropic

client = Anthropic(base_url="https://app.keyala.io/v1/anthropic", api_key=GRANT_TOKEN, max_retries=0)
reply = client.messages.create(
    model="claude-haiku-5-5",
    max_tokens=256,
    messages=[{"role": "user", "content": "Hello!"}],
)
```

Anthropic SDK 會附加 `/v1/messages`；Keyala 同時提供 `/v1/anthropic/messages` 與 `/v1/anthropic/v1/messages`，兩條路徑皆接受 `x-api-key` 或 bearer token 中的授權權杖。OpenRouter 授權使用 OpenAI 請求格式，路徑為 `/v1/openrouter/chat/completions`，OpenAI SDK 的 base URL 為 `https://app.keyala.io/v1/openrouter`，模型須用完整 slug，例如 `openai/gpt-4o-mini`。

直接使用 HTTP：

```sh
curl -i https://app.keyala.io/v1/openai/chat/completions \
  -H "Authorization: Bearer $KEYALA_GRANT_TOKEN" \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: job-2026-11-20-0001' \
  -H 'X-Keyala-Wait: 30' \
  -d '{"model":"gpt-4o-mini","max_tokens":256,"messages":[{"role":"user","content":"Hello!"}]}'
```

### 2.4 等待批准與重試

政策需要使用者決定時，Keyala 建立請求、通知手機，並**保持 HTTP 呼叫等待**。`X-Keyala-Wait` 指定等待秒數，預設 60、最多 120；超過會截至上限，`0` 則立即回傳。每個代理回應皆帶有 `X-Keyala-Request-Id`。

| 結果 | 意義 |
|---|---|
| `200` | 原樣回傳服務商回應。 |
| `202` `{"request_id":"req_...","status":"pending"}` | 尚未決定或仍在傳送。以同一 grant token 輪詢 `GET /v1/requests/{request_id}`；這不是模型回應。 |
| `403 keyala_denied` | 使用者拒絕請求。 |
| `410 keyala_expired` | 批准期限已過。 |
| `504 keyala_uncertain` | 服務商可能已收到請求；系統沒有重試。輪詢同一 request id，絕不可直接重新送出。 |
| 服務商錯誤狀態 | 回傳服務商的錯誤內容與狀態；沒有狀態時為 502。 |

```sh
curl https://app.keyala.io/v1/requests/REQUEST_ID -H "Authorization: Bearer $KEYALA_GRANT_TOKEN"
```

```json
{ "request_id": "req_...", "status": "completed", "response": { "...": "provider response" } }
```

狀態有 `pending`、`approved`、`sending`、`completed`、`denied`、`expired`、`failed`、`uncertain`。官方 SDK 可能將 202 視為非預期回應；需要處理此狀態時，請用原始 HTTP 或 SDK 的 raw-response API。

**重試。** 每筆儲存請求最多執行一次，但 HTTP 用戶端可能在回應遺失後重新送出 POST。請設定 `max_retries=0`（`maxRetries: 0`），或加上 `Idempotency-Key` header：1 至 128 個可見 ASCII 字元，每次邏輯呼叫使用唯一值，範圍限於該授權。同一 key 與相同內容會回傳原結果而不再次傳送；同一 key 與不同內容回傳 `409 idempotency_conflict`。

## 3. 核心概念

**授權與政策。** 授權連結單一使用者、你的應用與單一服務商。使用者選擇 `ask`（每次批准）、`count`（限定次數）、`budget`（金額上限）或 `trust`（不逐次詢問，可選每日警示）。對某應用的**首次**授權只能使用 `ask`；之後可在授權頁放寬。符合政策的呼叫直接通過，其餘等待批准。使用者可隨時撤銷，之後的呼叫回傳 `401 unauthorized`。應用名稱與瀏覽器 origin 由開發者提供，不是經驗證的身分；手機會顯示瀏覽器應用的 origin。

**預估與回報用量。** 呼叫前，Keyala 以請求 JSON 的 UTF-8 大小作為輸入 token 數，加上輸出上限，預留費用上限。這是安全預留，不是保證或預測。服務商回報 token 用量後，再據以計算實際費用；沒有用量資料時，費用維持未知，絕不顯示為零。所有金額都是服務商帳單的估算，不是發票。

**輸出上限。** `max_tokens` 依模型設上限，目前模型為 4096；省略時注入預設 1024。超過上限回傳 `400 max_tokens_too_large`。

**支援模型。** 僅支援文字、採允許清單，價格設定於伺服器 `backend/src/pricing.rs`。目前清單：

| 服務商 | 模型名稱 |
|---|---|
| OpenAI | `gpt-4o`, `gpt-4o-mini`, `gpt-4.1`, `gpt-4.1-mini` |
| Anthropic | `claude-opus-5-5`, `claude-sonnet-5-5`, `claude-haiku-5-5` |
| OpenRouter | `openai/gpt-4o`, `openai/gpt-4o-mini`, `openai/gpt-4.1`, `openai/gpt-4.1-mini`, `anthropic/claude-opus-5.5`, `anthropic/claude-sonnet-5.5`, `anthropic/claude-haiku-5.5` |

**不支援的選項會被拒絕，不會被忽略。** 允許清單以外的 key 回傳 `400 unsupported_option` 並指明 key；圖片、音訊、檔案、工具回傳 `400 unsupported_content`；`stream: true` 回傳 `400 streaming_unsupported`；`n` 不等於 1 也會被拒絕。OpenRouter 的 `models`、`provider`、`plugins` 會影響價格，因此不支援。

**保存期限。** 提示詞與回應內容於儲存時加密，在請求決定後 30 天移除，適用 completed、failed、denied、expired。模型、費用、狀態與時間等 metadata 保留；pending、approved、sending、uncertain 的內容保留至狀態解決。

**使用者可選的 OpenRouter 路由（`billing_vendor`）。** 使用者可按帳號開啟「將 OpenAI 與 Anthropic 應用經 OpenRouter 轉送」。應用的路徑、模型名稱、內容與回應格式不變。批准時，授權固定其 `billing_vendor`；轉送呼叫以對應 slug 與價格，使用該使用者的 OpenRouter key 計費。沒有已驗證映射的模型回傳 `400 model_not_routable`，不會改用直接金鑰。路由模式另拒絕 OpenAI 的 `max_completion_tokens`、`n`、`developer` role，以及 Anthropic 的 `metadata`、`top_k`。現有授權不變；使用者須重新配對才能變更路由。

## 4. 錯誤

錯誤皆為 JSON：`{"error":{"type":"...","message":"..."}}`。

| `type` | HTTP | 意義 | 處理方式 |
|---|---|---|---|
| `unauthorized` | 401 | 權杖缺少、無效、授權已撤銷，或瀏覽器配對未提供 app secret 或 app id | 檢查權杖；授權撤銷時請使用者重新配對 |
| `invalid_credentials` | 401 | 帳號或密碼錯誤，僅限營運者登入 | 應用不適用 |
| `vendor_mismatch` | 403 | 授權的服務商與路徑不同 | 使用符合授權的路徑 |
| `origin_not_allowed` | 403 | 瀏覽器來源不在 `allowed_origins` | 註冊完整 origin |
| `keyala_denied` | 403 | 使用者拒絕請求 | 停止並在介面顯示結果 |
| `not_found` | 404 | 找不到請求、配對碼或授權 | 檢查 id 與權杖 |
| `expired` | 410 | 配對碼過期，使用者端查詢 | 重新配對 |
| `keyala_expired` | 410 | 批准期限已過 | 讓使用者重新啟動操作 |
| `idempotency_conflict` | 409 | 相同 Idempotency-Key 搭配不同內容 | 新工作改用新 key |
| `invalid_request` | 400 | JSON 或欄位無效，例如空 `messages`、無效 `max_tokens`、`name` 或 `days` 不在 1 至 365 | 修正請求 |
| `model_not_allowed` | 400 | 模型不在該服務商的允許清單 | 使用清單內模型 |
| `model_not_priced` | 400 | 伺服器沒有已驗證的模型價格 | 使用其他模型或伺服器 |
| `model_not_routable` | 400 | 路由授權沒有 OpenRouter 模型映射 | 使用已映射模型 |
| `max_tokens_too_large` | 400 | 超過模型輸出上限 | 降低 `max_tokens` |
| `unsupported_option` | 400 | 不支援的請求 key 或值 | 移除該選項 |
| `unsupported_content` | 400 | 非文字內容或工具 | 僅送出純文字 |
| `streaming_unsupported` | 400 | `stream: true` | 停用串流 |
| `rate_limited` | 429 | 此位址註冊或配對過多 | 稍後再試 |
| `upstream_paused` | 503 | 營運者暫停服務商呼叫 | 稍後再試 |
| `data_key_unreadable` | 503 | 伺服器設定錯誤，主金鑰不符 | 由營運者處理 |
| `keyala_uncertain` | 504 | 服務商呼叫結果未知 | 輪詢 request id，絕不重新送出 |
| `internal` | 500 | 伺服器錯誤 | 重試輪詢，不要重送呼叫 |

## 5. 控制台與分析

開啟 Keyala 控制台 `https://app.keyala.io/console` 或 `https://staging.keyala.io/console`。用 Keyala App 掃描 QR code 並點選 **Approve web sign-in**，或使用帳號 email 與 App **Settings > Web password** 設定的密碼登入。網頁工作階段有效 12 小時，可讀取資料、撤銷授權與管理自己的應用；批准呼叫與修改金鑰仍須在手機操作。

在 **My apps** 可建立應用（secret 只顯示一次）、貼上 secret 認領現有應用、查看 7、30、90 天報表、輪替 secret、編輯名稱、首頁與允許來源，或刪除應用。報表包含呼叫、回報用量費用、未回報用量的完成呼叫、模型、國家及依檢視者時區顯示的星期與小時熱圖。舊版 `https://keyala.io/developers/` 仍可僅用 app secret 操作。

也可用 API 取得相同資料；依端點使用 app secret，或已登入且擁有該應用的帳號：

| 端點 | 驗證 | 用途 |
|---|---|---|
| `GET /v1/app/report?days=N` | `Bearer keyala_app_...` | secret 所對應應用的報表 |
| `GET /v1/me/apps` | session | 自己的應用與 7 天呼叫、費用 |
| `POST /v1/me/apps` `{name, homepage?, allowed_origins?}` | session | 建立自己擁有的應用，回傳一次 `app_secret` |
| `POST /v1/me/apps/claim` `{app_secret}` | session | 認領未被擁有的應用；他人已擁有時為 409 |
| `GET /v1/me/apps/{id}/report?days=N` | session | 格式同 `/v1/app/report`；非擁有者為 404 |
| `POST /v1/me/apps/{id}/secret` | session | 輪替 secret，舊 secret 立即失效 |
| `PUT /v1/me/apps/{id}` `{name, homepage?, allowed_origins?}` | session | 編輯；新連線採新設定，現有授權繼續有效 |
| `DELETE /v1/me/apps/{id}` | session | 刪除；secret 失效、所有授權撤銷、等待中呼叫拒絕，使用者保留歷史 |

```sh
curl 'https://app.keyala.io/v1/app/report?days=30' -H "Authorization: Bearer $KEYALA_APP_SECRET"
```

`days` 為 1 至 365 個 UTC 日，包含今天。僅提供彙總，不含提示詞、回應、使用者 id、email、IP 或金鑰。

```jsonc
{
  "app_id": "app_...", "days": 30, "from": "2026-10-22", "to": "2026-11-20",
  "totals": {
    "requests": 120, "completed": 100, "denied": 12, "expired": 2, "failed": 3, "uncertain": 1, "pending": 2,
    "cost_usd": 1.2345,        // from provider-reported usage, completed calls
    "estimated_usd": 2.1,      // pre-call estimates of approved calls
    "unmetered": 0,            // completed calls whose response had no usage
    "input_tokens": 90000, "output_tokens": 30000,
    "users": 14, "new_users": 6
  },
  "daily":     [{ "date": "2026-11-20", "requests": 7, "completed": 6, "cost_usd": 0.05 }],
  "models":    [{ "vendor": "openai", "model": "gpt-4o-mini", "requests": 80, "completed": 75, "cost_usd": 0.4, "input_tokens": 1, "output_tokens": 1 }],
  "hours_utc": [[0, 0, 3, "...24 values per row, 7 rows, row 0 = Sunday"]],
  "countries": [{ "country": "JP", "users": 5, "requests": 60, "cost_usd": 0.7 }]
}
```

`hours_utc` 依建立時的 UTC 星期與小時計數；顯示時轉為你的時區。`country` 是使用者批准配對時手機的兩碼國家代碼，由可信任的邊緣 header 提供，未知時為 `null`，不是伺服器所在地。secret 外洩時，以 `POST /v1/app/secret` 輪替；回傳新 `app_secret`，舊值立即失效。

## 6. API 參考（開發者與公開端點）

Base URL 為你的 Keyala 伺服器。開發者端點回應 CORS `Access-Control-Allow-Origin: *`，僅使用 bearer token。

| 方法 | 路徑 | 驗證 | 用途 |
|---|---|---|---|
| POST | `/v1/apps` | 無，有速率限制 | 註冊：`{name, homepage?, icon_url?, allowed_origins?}` 回傳 `{app_id, app_secret}` |
| GET | `/v1/app` | app secret | 應用名稱、首頁、圖示、`allowed_origins`、`created_at` |
| POST | `/v1/app/secret` | app secret | 輪替 secret |
| GET | `/v1/app/report?days=30` | app secret | 用量報表，見第 5 節 |
| POST | `/v1/connect` | app secret，或允許來源瀏覽器的 app id | 開始配對：`{vendor, suggested_policy?, app_id?}` |
| GET | `/v1/connect/{code}` | app secret 或 `connect_secret` | 輪詢；批准後只回傳一次 grant token |
| GET | `/connect/{code}/qr.svg` | 無 | 配對連結的 QR 圖片 |
| GET | `/sdk/keyala.js` | 無 | 網頁 SDK |
| POST | `/v1/openai/chat/completions` | grant | OpenAI 相容文字呼叫 |
| POST | `/v1/anthropic/messages` | grant，bearer 或 `x-api-key` | Anthropic 相容文字呼叫 |
| POST | `/v1/anthropic/v1/messages` | grant，bearer 或 `x-api-key` | 相同功能，供官方 Anthropic SDK 使用 |
| POST | `/v1/openrouter/chat/completions` | grant | OpenRouter 呼叫，使用 OpenAI 格式 |
| GET | `/v1/requests/{id}` | grant | 此授權某筆請求的狀態與回應 |
| GET | `/v1/status` | 無 | `{ok, paused, db, version}` |
| GET | `/healthz` | 無 | 存活檢查：`ok` |

`/v1/me/*` 屬於 Keyala 使用者 App，處理金鑰、批准、授權、歷史與帳號。使用使用者 session、不提供 CORS，不屬於開發者 API。

## 7. 自行架設簡介

伺服器是一個 Rust 執行檔與一份 SQLite 檔，原始碼位於 repo 的 `backend/`。模擬服務商可在沒有真實金鑰或 credits 時開發。

```sh
export KEYALA_MASTER_KEY=$(openssl rand -base64 32)   # encrypts every stored key; keep it safe
KEYALA_DEV_LOGIN=1 KEYALA_VENDOR_MOCK=1 cargo run     # http://localhost:8080
```

| 變數 | 意義 |
|---|---|
| `KEYALA_MASTER_KEY` | 必填，32 bytes 的 Base64。遺失即無法解開所有已儲存金鑰。 |
| `PORT`（預設 8080）、`KEYALA_PUBLIC_URL` | 監聽連接埠及 `connect_url` 的基底。 |
| `KEYALA_DATABASE_URL` | SQLite 檔案位置。 |
| `KEYALA_VENDOR_MOCK=1` | 模擬回應；訊息含 `KEYALA_MOCK_FAIL` 時回傳 500。 |
| `KEYALA_DEV_LOGIN=1` | 測試登入端點；不可在公開伺服器啟用。 |
| `KEYALA_PRICES_VERIFIED=1` | 營運者已確認價格表；否則未驗證模型拒絕執行。 |
| `KEYALA_PUBLIC_APPS` | 啟動時宣告的瀏覽器應用 JSON 清單，含 id、name、allowed origins。 |
| `KEYALA_COUNTRY_HEADER` | 可信任的邊緣國家 header，用於國家彙總報表。 |
| `KEYALA_PAUSE_UPSTREAM=1` | 暫停新服務商呼叫，也可用 `PAUSE_UPSTREAM` 檔。 |
| `KEYALA_BACKUP_DIR`、`KEYALA_BACKUP_KEEP`、`KEYALA_BACKUP_INTERVAL_SECS` | 程序內 SQLite 備份設定。 |

完整設定及完整流程的 curl 範例請見 `backend/README.md`。

## 8. 限制與信任邊界

- 個人營運的實驗性 beta，無 SLA；免費代管測試伺服器可能重設資料。
- Beta 期間免費，包含控制台。模型費用由服務商向使用者自己的帳號收取；ChatGPT 或 Claude 訂閱不包含 API credits。
- 費用為預估與預留上限，不是服務商發票；缺少用量資料時為未知，絕不視為零。
- 伺服器營運者可解密儲存的金鑰以轉送已授權呼叫，因此使用者必須信任營運者與服務商。你的應用不會收到金鑰。
- 僅支援文字、無串流、固定模型允許清單；不支援的功能會明確失敗，不會默默改變請求。
- 刪除帳號會移除線上金鑰、授權與歷史；備份依保存期限移除，不會立即消除。
