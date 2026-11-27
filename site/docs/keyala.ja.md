# Keyala 開発者ドキュメント

Keyala を使うと、利用者自身の OpenAI、Anthropic、OpenRouter の API キーで AI アプリを動かせます。アプリがキーを受け取ることはありません。利用者が一度接続を許可すると、アプリには範囲を限定した許可トークンが渡されます。呼び出しはスマートフォンで承認するか、利用上限を設定します。公式 SDK はそのまま使い、base URL を変更します。この Markdown が開発者向けの全文で、AI コーディング支援にもそのまま渡せます。

ホスト先は本番 `https://app.keyala.io`、ステージング `https://staging.keyala.io` です。既存のコア API は両方で利用できます。今回のクーポン API は staging のみで検証します。登録、接続、呼び出し、レポートには同じサーバーを使ってください。Keyala は無料の実験的ベータ版です。制限は最終節を参照してください。

## スポンサーのクーポン（staging で検証）

クーポンは staging で検証中です。本番での公開を意味しません。スマートフォン、Console、SDK は同じサーバーを使ってください。

Console にログインしてアプリを選び、バッチ専用のプロバイダーキー、プロバイダー、許可モデル、1 枚の利用枠、総予算、受け取り期限を設定します。有効期間は受け取った時点から数え、既定は 7 日です。開発者がプロバイダーに直接支払い、Keyala は利用枠の販売や代金の取り扱いをしません。スポンサーキーはサーバーで暗号化して保存し、利用者や接続先アプリに返しません。利用上限を別に設定した専用キーを使い、プロバイダーの規約と請求設定を確認してください。Keyala の上限は、プロバイダーの請求額を保証するものではありません。

バッチの `/coupon/KC-…` リンクや QR を共有します。利用者はログインすれば、自分のキーを追加せずに確認して受け取れます。1 アカウントにつき同じバッチは 1 回限りで、期限切れ後も再取得できません。受け取った時点で 1 枚分を総予算から確保します。リンクの共有だけでは予算を使いません。未割り当て予算が 1 枚分に足りなくなると新規取得を停止し、取得済みの利用枠は維持します。期限切れや取り消しで未使用分を一度だけ戻しますが、実行中と結果不明の予約は残します。

`POST /v1/connect` の `coupon_code`、または Web SDK のクーポンオプションで支払い元を選びます。接続するアプリとプロバイダーが一致している必要があります。取得だけでアプリを許可したことにはなりません。利用者は接続と**毎回の呼び出し**を承認し、プロンプト全文、記録された応答、支払い元、残額、期限を確認できます。クーポンでは回数、予算、信頼による自動承認へ変更できません。許可モデルはサーバーで確認します。プロバイダー向けリクエスト形式とポーリングは従来どおりです。

期限切れ、取り消し、概算費用に対する残額不足では、理由を示して停止します。**個人のキーへ自動では切り替えません。** 自分で支払う場合は、明示的に別の BYOK 接続を作成します。

バッチレポートは取得数、初回成功者数、リクエストとモデル別支出の集計だけを示し、利用者の身元、プロンプト、応答を公開しません。実費、概算、結果不明の予約は区別します。メールは新規割り当て枠の不足と総支出枠の消尽を発行者に知らせるためのもので、支出制御には使いません。運営者が設定するまでメールは無効です。今回はモデルとメールの mock 検証で、実際の有料呼び出しや受信箱への配信は未検証です。

| メソッド | パス | 用途 |
|---|---|---|
| GET / POST | `/v1/me/apps/{app_id}/coupons` | バッチ作成／一覧 |
| DELETE | `/v1/me/apps/{app_id}/coupons/{batch_id}` | バッチ取り消し |
| GET | `/v1/me/apps/{app_id}/coupons/{batch_id}/report` | 集計レポート |
| GET / POST | `/v1/me/coupons/{code} / /v1/me/coupons/{code}/claim` | 確認／取得 |
| GET | `/v1/me/coupons` | 自分のクーポン一覧 |

発行側はログイン済みのアプリ所有者、取得側はログイン済みの利用者だけが呼び出せます。作成時の本文：

```json
{"vendor":"openai","models":["gpt-4o-mini"],"api_key":"REPLACE_WITH_DEDICATED_PROVIDER_KEY","per_coupon_usd":1,"total_usd":20,"validity_days":7,"claim_deadline":"2026-12-31T23:59:59.000Z"}
```

```js
const { grant_token } = await Keyala.connect({
  app: "app_your_app", vendor: "openai", coupon: "KC-REPLACE_WITH_BATCH_CODE"
});
```

## 1. Keyala と呼び出しの流れ

アプリが Keyala に利用者との接続を要求し、利用者が Keyala iOS アプリで承認すると、利用者、アプリ、単一のプロバイダーに結び付いた `grant_token` が渡されます。アプリはこのトークンで通常のプロバイダー向けリクエストを送ります。Keyala はポリシーを確認し、概算費用を予約し、必要に応じてスマートフォンで承認を求めます。その後、保存された利用者のキーで転送し、プロバイダーのレスポンスをそのまま返します。

```
your app --grant token--> Keyala server --(ask user if policy says so)--> user's phone
                              |  approved                                  approve / deny
                              v
                    provider (OpenAI / Anthropic / OpenRouter), user's key
                              |
your app <---- provider response, unchanged ---+
```

## 2. クイックスタート（5 分）

### 2.1 アプリを登録する

```sh
curl https://app.keyala.io/v1/apps \
  -H 'Content-Type: application/json' \
  -d '{"name":"My AI app","homepage":"https://yourapp.example"}'
```

```json
{ "app_id": "app_...", "app_secret": "keyala_app_..." }
```

`app_secret` は一度だけ表示されます。サーバー側のシークレット管理に保存し、ブラウザーのコード、リポジトリ、URL、ログには置かないでください。登録は送信元アドレスごとに 1 時間 10 回までで、超えると `429 rate_limited` です。`name` は必須で最大 100 文字です。

ブラウザーだけで動くアプリは `allowed_origins` も送ります。`["https://yourapp.example","http://localhost:3000"]` のように、スキームとホストを含みパスを含まない正確な origin を 1〜10 個指定します。`http://` は localhost のみです。`app_id` は公開でき、ページのコードに置けますが、secret は置けません。secret のローテーションや紛失で利用者のキーが漏れることはありません。

### 2.2 利用者と接続する

コードを実行する場所に合わせて選びます。

**サーバーアプリ。** app secret を使います。

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

`vendor` は `openai`、`anthropic`、`openrouter` のいずれかです。QR コード（`connect_qr_url` は Keyala が提供する SVG）、またはリンクとコードを表示します。コードは 10 分間有効です。同じ secret でポーリングします。

```sh
curl https://app.keyala.io/v1/connect/KY-4F7Q -H "Authorization: Bearer $KEYALA_APP_SECRET"
```

`status` は `pending`、`approved`、`denied`、`expired` です。待機中には `seen` が付き、`null`、`"web"`（ブラウザーでリンクを開いたため、アプリ未導入の可能性あり）、`"app"`（スマートフォンで確認中）を示します。承認時の `{"status":"approved","grant_token":"keyala_grant_..."}` は**一度だけ**返ります。次のポーリング前に保存してください。失った場合は接続をやり直します。

**ブラウザーアプリ。** `allowed_origins` を登録し、同じサーバーが提供する Web SDK を使います。パソコンでは QR とコード、スマートフォンでは「Keyala を開く」ボタンのダイアログが表示され、成功すると許可を返します。

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

`Keyala.connect({ app, vendor, lang? })` は失敗時に `err.code` として `cancelled`、`denied`、`unreachable`、`failed` を返します。ボタン形式の `<keyala-connect app="app_..." vendor="openai"></keyala-connect>` も使えます。`connected` イベントの `detail.grant_token` でトークンを取得します。SDK はトークンを保存しません。保存方法を意図的に設計していない場合はメモリーに保持してください。SDK を使わない場合、許可された `Origin` から、Authorization ヘッダーなしで `POST /v1/connect` に `{"app_id":"app_...","vendor":"openai"}` を送ります。未許可の origin は `403 origin_not_allowed`、アドレスごとに 10 分間 20 コードまでです。レスポンスには `connect_secret`（`keyala_cs_...`）が追加され、`GET /v1/connect/{code}` のポーリングに使います。

### 2.3 呼び出す

grant token を API キー、Keyala サーバーを base URL として使います。SDK の自動再試行は無効にしてください（2.4 を参照）。

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

Anthropic SDK は `/v1/messages` を付加します。Keyala は `/v1/anthropic/messages` と `/v1/anthropic/v1/messages` の両方を提供し、どちらも `x-api-key` または bearer token で許可トークンを受け付けます。OpenRouter の許可は OpenAI 形式で `/v1/openrouter/chat/completions` を使います。OpenAI SDK の base URL は `https://app.keyala.io/v1/openrouter`、モデルは `openai/gpt-4o-mini` のような完全な slug です。

HTTP を直接使う場合：

```sh
curl -i https://app.keyala.io/v1/openai/chat/completions \
  -H "Authorization: Bearer $KEYALA_GRANT_TOKEN" \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: job-2026-11-20-0001' \
  -H 'X-Keyala-Wait: 30' \
  -d '{"model":"gpt-4o-mini","max_tokens":256,"messages":[{"role":"user","content":"Hello!"}]}'
```

### 2.4 承認待ち、待機時間、再試行

ポリシー上、利用者の判断が必要な場合、Keyala はリクエストを作成し、スマートフォンに通知して **HTTP 呼び出しを保持**します。`X-Keyala-Wait` は保持する秒数で、既定 60、最大 120 です。超過値は上限に制限され、`0` は直ちに返ります。各プロキシレスポンスに `X-Keyala-Request-Id` が付きます。

| 結果 | 意味 |
|---|---|
| `200` | プロバイダーのレスポンスをそのまま返します。 |
| `202` `{"request_id":"req_...","status":"pending"}` | 未決定または送信中です。同じ grant token で `GET /v1/requests/{request_id}` をポーリングしてください。モデルのレスポンスではありません。 |
| `403 keyala_denied` | 利用者が拒否しました。 |
| `410 keyala_expired` | 承認期限を過ぎました。 |
| `504 keyala_uncertain` | プロバイダーが受信した可能性があり、再試行はしていません。同じ request id をポーリングし、再送しないでください。 |
| プロバイダーのエラーステータス | エラー本文とステータスを返します。ステータスがなければ 502 です。 |

```sh
curl https://app.keyala.io/v1/requests/REQUEST_ID -H "Authorization: Bearer $KEYALA_GRANT_TOKEN"
```

```json
{ "request_id": "req_...", "status": "completed", "response": { "...": "provider response" } }
```

状態は `pending`、`approved`、`sending`、`completed`、`denied`、`expired`、`failed`、`uncertain` です。公式 SDK は 202 を想定外のレスポンスとして扱う場合があります。必要に応じて生の HTTP または raw-response API を使ってください。

**再試行。** 保存された各リクエストの実行は最大一度ですが、HTTP クライアントがレスポンス喪失時に POST を再送する場合があります。`max_retries=0`（`maxRetries: 0`）を設定するか、`Idempotency-Key` ヘッダーを送ってください。1〜128 文字の表示可能な ASCII を、論理的な呼び出しごとに一意に指定します。有効範囲はその許可内です。同じキーと同じ本文なら元の結果を返し、新たに送信しません。同じキーと異なる本文なら `409 idempotency_conflict` です。

## 3. 概念

**許可とポリシー。** 許可は一人の利用者、アプリ、一つのプロバイダーを結びます。利用者は `ask`（毎回承認）、`count`（回数）、`budget`（金額上限）、`trust`（毎回の確認なし、任意の日次アラート）を選びます。アプリへの**初回**の許可は `ask` のみで、その後 Grants から変更できます。条件内の呼び出しは直接転送され、それ以外は承認を待ちます。いつでも取り消せ、次の呼び出しは `401 unauthorized` になります。アプリ名とブラウザーの origin は開発者の申告で、検証済みの身元ではありません。ブラウザーアプリの origin はスマートフォンに表示されます。

**概算と報告された使用量。** 呼び出し前に、リクエスト JSON の UTF-8 サイズを入力 token 数とし、出力上限を加えた費用の上限を予約します。安全のための予約で、保証や予測ではありません。プロバイダーが token 使用量を返した場合、その値から実費を計算します。使用量がなければ費用は不明のままで、ゼロとは表示しません。金額はすべてプロバイダー請求の概算で、請求書ではありません。

**出力上限。** `max_tokens` はモデルごとに制限され、現在のモデルは 4096 です。省略時は 1024 を補います。上限超過は `400 max_tokens_too_large` です。

**対応モデル。** テキストのみ、許可リスト方式です。価格はサーバーの `backend/src/pricing.rs` にあります。現在の一覧：

| プロバイダー | モデル名 |
|---|---|
| OpenAI | `gpt-4o`, `gpt-4o-mini`, `gpt-4.1`, `gpt-4.1-mini` |
| Anthropic | `claude-opus-5-5`, `claude-sonnet-5-5`, `claude-haiku-5-5` |
| OpenRouter | `openai/gpt-4o`, `openai/gpt-4o-mini`, `openai/gpt-4.1`, `openai/gpt-4.1-mini`, `anthropic/claude-opus-5.5`, `anthropic/claude-sonnet-5.5`, `anthropic/claude-haiku-5.5` |

**非対応のオプションは無視せず拒否します。** 明示的な許可リスト外のキーは名前付きの `400 unsupported_option`、画像、音声、ファイル、ツールは `400 unsupported_content`、`stream: true` は `400 streaming_unsupported` です。`n` が 1 以外の場合も拒否します。OpenRouter の `models`、`provider`、`plugins` は価格を変えるため拒否します。

**保存期間。** プロンプトとレスポンスは保存時に暗号化し、リクエストの決定後 30 日で削除します。completed、failed、denied、expired が対象です。モデル、費用、状態、日時の metadata は残ります。pending、approved、sending、uncertain は解決まで内容を保持します。

**利用者が選べる OpenRouter 経由の転送（`billing_vendor`）。** 利用者はアカウント単位で「OpenAI と Anthropic のアプリを OpenRouter 経由にする」を有効にできます。アプリ側のルート、モデル名、本文、レスポンス形式は変わりません。承認時に許可の `billing_vendor` が固定され、対応 slug と価格で利用者の OpenRouter キーに課金されます。検証済みの対応モデルがない場合は `400 model_not_routable` となり、直接キーへのフォールバックはしません。転送時は OpenAI の `max_completion_tokens`、`n`、`developer` role と Anthropic の `metadata`、`top_k` も拒否します。既存の許可は変わりません。転送方法を変えるには再接続が必要です。

## 4. エラー

すべて JSON 形式です：`{"error":{"type":"...","message":"..."}}`。

| `type` | HTTP | 意味 | 対応 |
|---|---|---|---|
| `unauthorized` | 401 | トークンなし、無効、許可取消済み、または app secret と app id のないブラウザー接続 | トークンを確認し、取消済みなら再接続を依頼 |
| `invalid_credentials` | 401 | ユーザー名またはパスワードが不正。運営者ログインのみ | アプリには該当しません |
| `vendor_mismatch` | 403 | 許可とルートのプロバイダーが不一致 | 許可に合ったルートを使用 |
| `origin_not_allowed` | 403 | origin が `allowed_origins` にない | 正確な origin を登録 |
| `keyala_denied` | 403 | 利用者が拒否 | 停止し、UI に表示 |
| `not_found` | 404 | リクエスト、接続コード、許可が不明 | id とトークンを確認 |
| `expired` | 410 | 接続コードが期限切れ。利用者側の照会 | 新たに接続 |
| `keyala_expired` | 410 | 承認期限切れ | 利用者に操作をやり直してもらう |
| `idempotency_conflict` | 409 | 同じ Idempotency-Key で異なる本文 | 新しい処理には新しいキーを使用 |
| `invalid_request` | 400 | 不正な JSON やフィールド。空の `messages`、不正な `max_tokens`、`name`、1〜365 の範囲外の `days` など | リクエストを修正 |
| `model_not_allowed` | 400 | プロバイダーの許可リスト外モデル | 一覧内のモデルを使用 |
| `model_not_priced` | 400 | 検証済み価格なし | 別のモデルかサーバーを使用 |
| `model_not_routable` | 400 | OpenRouter の対応モデルなし | 対応モデルを使用 |
| `max_tokens_too_large` | 400 | モデルの出力上限超過 | `max_tokens` を減らす |
| `unsupported_option` | 400 | 非対応のキーまたは値 | オプションを除去 |
| `unsupported_content` | 400 | テキスト以外またはツール | プレーンテキストのみ送信 |
| `streaming_unsupported` | 400 | `stream: true` | ストリーミングを無効化 |
| `rate_limited` | 429 | このアドレスからの登録や接続が過多 | 時間を置いて再試行 |
| `upstream_paused` | 503 | 運営者が呼び出しを一時停止 | 時間を置いて再試行 |
| `data_key_unreadable` | 503 | 主キー不一致などサーバー設定の問題 | 運営者が対応 |
| `keyala_uncertain` | 504 | 呼び出しの結果が不明 | request id をポーリングし、再送しない |
| `internal` | 500 | サーバーエラー | 呼び出しではなくポーリングを再試行 |

## 5. ダッシュボードと分析

Keyala コンソール `https://app.keyala.io/console` または `https://staging.keyala.io/console` を開きます。Keyala アプリで QR を読み取り **Approve web sign-in** を押すか、アカウントのメールとアプリの **Settings > Web password** で設定したパスワードでログインします。Web セッションは 12 時間有効で、読み取り、許可取消、自分のアプリの管理に使えます。呼び出しの承認とキーの変更はスマートフォンで行います。

**My apps** では、アプリの作成（secret は一度だけ表示）、secret を貼り付けた既存アプリの取得、7・30・90 日のレポート、secret のローテーション、名前・ホームページ・許可 origin の編集、削除ができます。レポートには呼び出し、報告された使用量の費用、使用量報告のない完了呼び出し、モデル、国、閲覧者のタイムゾーンによる曜日と時間のヒートマップが含まれます。旧ページ `https://keyala.io/developers/` は app secret だけでも利用できます。

同じデータは API でも取得できます。エンドポイントに応じて app secret、またはそのアプリを所有するログイン済みアカウントを使います。

| エンドポイント | 認証 | 用途 |
|---|---|---|
| `GET /v1/app/report?days=N` | `Bearer keyala_app_...` | secret に対応するアプリのレポート |
| `GET /v1/me/apps` | session | 自分のアプリと 7 日間の呼び出し・費用 |
| `POST /v1/me/apps` `{name, homepage?, allowed_origins?}` | session | 自分のアプリを作成。`app_secret` は一度だけ返す |
| `POST /v1/me/apps/claim` `{app_secret}` | session | 未所有アプリを取得。他人が所有済みなら 409 |
| `GET /v1/me/apps/{id}/report?days=N` | session | `/v1/app/report` と同形式。所有者以外は 404 |
| `POST /v1/me/apps/{id}/secret` | session | ローテーション。古い secret は即時無効 |
| `PUT /v1/me/apps/{id}` `{name, homepage?, allowed_origins?}` | session | 編集。新規接続には新設定、既存の許可は維持 |
| `DELETE /v1/me/apps/{id}` | session | 削除。secret 無効、全許可取消、待機呼び出し拒否。利用者の履歴は維持 |

```sh
curl 'https://app.keyala.io/v1/app/report?days=30' -H "Authorization: Bearer $KEYALA_APP_SECRET"
```

`days` は今日を含む UTC の 1〜365 日です。集計のみで、プロンプト、レスポンス、利用者 id、メール、IP、キーは含みません。

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

`hours_utc` は作成時の UTC 曜日と時間による件数です。表示時に自分のタイムゾーンへ変換してください。`country` は接続承認時のスマートフォンの 2 文字国コードで、信頼するエッジヘッダー由来です。取得不能なら `null` で、サーバーの所在地ではありません。secret が漏れたら `POST /v1/app/secret` でローテーションします。新しい `app_secret` が返り、古い値は即時無効になります。

## 6. API リファレンス（開発者・公開エンドポイント）

Base URL は Keyala サーバーです。開発者エンドポイントは CORS `Access-Control-Allow-Origin: *` を返し、bearer token のみを使います。

| メソッド | パス | 認証 | 用途 |
|---|---|---|---|
| POST | `/v1/apps` | なし、レート制限あり | 登録：`{name, homepage?, icon_url?, allowed_origins?}` から `{app_id, app_secret}` |
| GET | `/v1/app` | app secret | 名前、ホームページ、アイコン、`allowed_origins`、`created_at` |
| POST | `/v1/app/secret` | app secret | secret のローテーション |
| GET | `/v1/app/report?days=30` | app secret | 使用量レポート（第 5 節） |
| POST | `/v1/connect` | app secret、または許可 origin のブラウザーの app id | 接続開始：`{vendor, suggested_policy?, app_id?}` |
| GET | `/v1/connect/{code}` | app secret または `connect_secret` | ポーリング。承認後に grant token を一度だけ返す |
| GET | `/connect/{code}/qr.svg` | なし | 接続リンクの QR 画像 |
| GET | `/sdk/keyala.js` | なし | Web SDK |
| POST | `/v1/openai/chat/completions` | grant | OpenAI 互換テキスト呼び出し |
| POST | `/v1/anthropic/messages` | grant、bearer または `x-api-key` | Anthropic 互換テキスト呼び出し |
| POST | `/v1/anthropic/v1/messages` | grant、bearer または `x-api-key` | 公式 Anthropic SDK 用の同じ機能 |
| POST | `/v1/openrouter/chat/completions` | grant | OpenAI 形式の OpenRouter 呼び出し |
| GET | `/v1/requests/{id}` | grant | この許可に属するリクエストの状態と応答 |
| GET | `/v1/status` | なし | `{ok, paused, db, version}` |
| GET | `/healthz` | なし | 生存確認：`ok` |

`/v1/me/*` は Keyala 利用者アプリ用で、キー、承認、許可、履歴、アカウントを扱います。利用者 session を使い、CORS はなく、開発者 API の対象外です。

## 7. セルフホストの概要

サーバーは Rust の実行ファイル一つと SQLite ファイル一つです。ソースはリポジトリの `backend/` にあります。モックプロバイダーを使えば、実際のキーやクレジットなしで開発できます。

```sh
export KEYALA_MASTER_KEY=$(openssl rand -base64 32)   # encrypts every stored key; keep it safe
KEYALA_DEV_LOGIN=1 KEYALA_VENDOR_MOCK=1 cargo run     # http://localhost:8080
```

| 変数 | 意味 |
|---|---|
| `KEYALA_MASTER_KEY` | 必須。32 bytes の Base64。失うと保存された全キーを復号できません。 |
| `PORT`（既定 8080）、`KEYALA_PUBLIC_URL` | 待受ポートと `connect_url` のベース。 |
| `KEYALA_DATABASE_URL` | SQLite ファイルの場所。 |
| `KEYALA_VENDOR_MOCK=1` | モック応答。メッセージに `KEYALA_MOCK_FAIL` があると 500。 |
| `KEYALA_DEV_LOGIN=1` | テストログイン用。公開サーバーでは有効にしないでください。 |
| `KEYALA_PRICES_VERIFIED=1` | 運営者が価格表を確認済み。未確認モデルは実行を拒否。 |
| `KEYALA_PUBLIC_APPS` | 起動時に宣言するブラウザーアプリの JSON リスト。id、name、allowed origins。 |
| `KEYALA_COUNTRY_HEADER` | 国別集計に使う、信頼するエッジの国ヘッダー。 |
| `KEYALA_PAUSE_UPSTREAM=1` | 新規呼び出しを停止。`PAUSE_UPSTREAM` ファイルでも指定可能。 |
| `KEYALA_BACKUP_DIR`、`KEYALA_BACKUP_KEEP`、`KEYALA_BACKUP_INTERVAL_SECS` | プロセス内の SQLite バックアップ設定。 |

全設定と一連の curl 手順は `backend/README.md` を参照してください。

## 8. 制限と信頼の前提

- 個人運営の実験的ベータ版で、SLA はありません。無料のステージングサーバーはデータをリセットする場合があります。
- ベータ期間はダッシュボードを含め無料です。モデル使用料は利用者自身のプロバイダーアカウントに請求されます。ChatGPT や Claude のサブスクリプションに API クレジットは含まれません。
- 費用は概算と予約上限で、プロバイダーの請求書ではありません。使用量不明はゼロではありません。
- サーバー運営者は、許可された呼び出しの転送のため保存キーを復号できます。利用者は運営者とプロバイダーを信頼する必要があります。アプリはキーを受け取りません。
- テキストのみ、ストリーミングなし、固定のモデル許可リストです。非対応機能は黙って変更せず、明示的に失敗します。
- アカウント削除で稼働中のキー、許可、履歴は削除されます。バックアップは即時消去ではなく、保存期間の終了後に削除されます。
