# Keyala beta tester guide

[English](#english) | [繁體中文](#繁體中文) | [日本語](#日本語)

[三語邀請文](#邀請-email-文面寄送前填寫方括號) | [Joseph 自測清單](#joseph-自測清單手動填結果)

This guide accompanies a TestFlight invitation. Applying on the website does not install the app or guarantee an invitation. Use the build identified in your invitation; screenshots on the website may show sample data.

## English

### Before you start

- An iPhone running iOS 17 or later, an Apple Account, and Apple's TestFlight app.
- Install Apple’s TestFlight from the App Store first. Then open Apple’s Keyala invitation email on your iPhone: View in TestFlight → Accept → Install. Developer Mode and a computer are not required for TestFlight installation.
- About 10–15 minutes. A second screen is useful for scanning a connection QR; on the same phone, use the connection link or KY- code instead.
- A provider account with API credit only if you choose to test a real model call. Keyala beta access is free; provider usage is billed separately to your provider account. ChatGPT or Claude app subscriptions do not themselves supply API credit.

Before opening Keyala, check its version and build in TestFlight against your invitation. You can also find App version in Keyala → Settings → Help & contact. If the build differs, report it before continuing. Sign in with your own Apple Account; public testers must not use Apple reviewer credentials.

You can inspect the interface without a provider key: Settings → Developer options → enable developer options → Use sample data. Samples are offline and do not prove that sign-in, pairing, notifications, or billing works. Switch samples off before the real flow; switching this setting signs you out. A separate server mock can exercise an integration without provider charges, but only on an explicitly confirmed mock server with fake keys. The sample switch does not put production into mock mode. Public testers should stay on production and use offline samples unless they choose paid provider testing; Apple reviewer access is not a public mock option.

### Your first test

For offline samples only, stop after exploring the sample interface. The steps below are an optional connected test and can incur provider charges on production.

1. Open Keyala, read the introduction, choose a language, and sign in with Apple. Fresh installs use the production server automatically. Leave custom server settings unchanged for this test.
2. If testing real calls, connect one provider in Keys. Use Connect OpenRouter or paste your own provider key. Review the provider's consent and billing information. A spending limit is not free credit. Never send a key to us as feedback.
3. Open the test tool named in your invitation. Use fictional text, for example: “Help me practise a short interview introduction for a fictional bookstore job.” Start its Keyala connection.
4. Scan its connection QR in Keyala, or open its connection link on the phone. If the link does not open the app, use the KY- code. Check the app name, server, provider, and billing route before connecting. Keep the initial policy at Ask.
5. Send one short text request from that tool. Keep Keyala in the foreground for the first attempt. In Inbox, inspect the messages, model, estimated cost and any OpenRouter billing route; choose This call only. Confirm the tool receives a response and Activity shows the result. Estimates and usage-derived costs are not a provider invoice.
6. Send a second request and choose Deny. Confirm that the tool reports the denial. If a request is uncertain, do not repeatedly resubmit it: check history and report the request ID privately.
7. In Apps, open that tool → Disconnect app → confirm to revoke its access. Already-sent requests may still finish. Confirm it requires a new connection for another call. Try changing the language and reopening Keyala; the preference should remain.

If a step fails, stop there and report it. You do not need to spend more to reproduce it. No provider key or credit is required to report a failure.

### Optional tests and stopping

- Allow notifications, lock the phone, and trigger one request. Record whether a notification arrives and whether tapping it opens the correct request. If none arrives, reopen Keyala; foreground refresh is a separate path.
- Cancel device verification once: approval should not be sent. Camera permission denial should still leave code entry available.
- Removing a key in Keyala does not revoke the key at the provider. To end access, revoke the app permission and remove the key; revoke the provider key at its official console if needed.
- Settings → Account offers account deletion with fresh verification. This is irreversible: use it only if you intend to delete the beta account, and do not recreate it merely for testing. Backups can retain older data until they expire; the privacy page explains handling.

### Feedback

Open TestFlight → Keyala → Send Beta Feedback. Choose Include Screenshot and select a redacted image, or Don’t Include Screenshot; add the report below and tap Submit. Apple also receives TestFlight feedback. Alternatively use [Keyala support](https://keyala.io/support/). Include only what is needed:

```text
Keyala version and build (shown in TestFlight):
iPhone model / iOS version / app language:
Date, time and time zone:
Steps to reproduce (numbered):
What I expected / what happened:
Can it happen again? (No need to spend more to check.)
Provider and model, if relevant:
Request ID, if relevant (private support only):
Screenshot with sensitive information hidden, optional:
```

Never include API keys, passwords, session/grant tokens, pairing QR/code, private prompts/responses, or unredacted account details. Check screenshots before sending. The signup form is for applications, not bug reports.

## 繁體中文

### 開始前

準備 iOS 17 以上的 iPhone 和自己的 Apple 帳號。先從 App Store 安裝 Apple TestFlight，再開啟 Apple 寄來的 Keyala 邀請 email，點 View in TestFlight → Accept → Install。不需要開啟開發者模式。網站報名不等於已收到邀請。預留 10–15 分鐘；另一個螢幕方便掃 QR，同一支手機可改用連結或 KY- 代碼。

安裝後先在 TestFlight 核對版本與 build 是否符合邀請；App 內也可從 Settings → Help & contact 查看 App version。不同就先回報。公開測試者請用自己的 Apple 帳號登入，不要使用 Apple 審查專用帳密。

Keyala 測試版免費；真實模型呼叫會使用你自己的服務商 API 額度，另外計費。ChatGPT／Claude App 訂閱本身不等於 API 額度。只想看介面，可到 Settings → Developer options 啟用開發者選項，再開 Use sample data；這是離線範例，不能驗證登入、配對、推播或計費。實測前請關閉範例資料；切換會登出。伺服器 mock 可用假 key 測完整流程且不產生服務商費用，但必須先確認該伺服器確實使用 mock；離線樣本開關不會讓正式服務變成 mock。公開測試請維持正式服務，選離線樣本或自行決定是否付費實測，不使用 Apple reviewer 登入。

### 首次測試

只看離線樣本可在體驗介面後結束。以下是自選的連線測試，在正式服務可能產生服務商費用。

1. 開啟 Keyala、閱讀說明、選擇語言，以 Apple 登入。全新安裝預設連正式服務，不需修改伺服器。
2. 若要測真實呼叫，在 Keys 連接一個服務商：可連 OpenRouter 或貼上自己的 API key。先確認同意畫面、付款與額度；支出上限不是免費額度。
3. 開啟邀請中指定的測試工具。只填虛構資料，例如「請幫我練習應徵虛構書店的一分鐘自我介紹」，開始 Keyala 連接。
4. 用 Keyala 掃工具的 QR，或在手機點連結；連結無法開 App 時可輸入 KY- 代碼。確認 App、伺服器、服務商及計費路徑，初始權限維持逐次詢問。
5. 從工具送出一個短文字請求，第一次先讓 Keyala 保持前景。在 Inbox 檢查訊息、模型、預估費用及 OpenRouter 計費資訊，只批准這一次。確認工具收到回應，Activity 有紀錄。預估與依用量計算的費用不是服務商帳單。
6. 第二次選拒絕，確認工具顯示被拒絕。遇到 uncertain 不要連續重送，先看歷史並私下回報 request ID。
7. 到 Apps 點開工具 → Disconnect app → 確認撤銷；已送出的請求仍可能完成。確認再次呼叫需要重新連接。切換語言、重開 App，確認語言保留。

失敗就停在該步回報，不必為了重現問題繼續花費。也可測通知、鎖定畫面點通知、取消裝置驗證，以及拒絕相機權限後輸入代碼。沒有通知時可先開 App 查看，前景更新與推播是不同路徑。

### 回報與結束測試

開啟 TestFlight → Keyala → Send Beta Feedback，選 Include Screenshot 加入已遮蔽的截圖，或選 Don’t Include Screenshot 不附圖；填寫下方範本後按 Submit。Apple 也會收到 TestFlight 回饋。也可透過 [支援頁](https://keyala.io/support/?lang=zh-Hant) 回報。附版本／build、iPhone／iOS、語言、時間與時區、操作步驟、預期與實際結果；需要時私下附 request ID。不要附 API key、密碼、token、配對 QR／代碼、私人對話或未遮蔽帳號資料。報名表不用來回報 bug。

```text
Keyala 版本／build：
iPhone 機型／iOS 版本／App 語言：
日期、時間與時區：
重現步驟（依序列出）：
預期結果：
實際結果：
是否曾再次發生（不必額外付費重現）：
服務商／模型（如相關）：
request ID（如相關，只交給私人支援渠道）：
已遮蔽敏感內容的截圖（可省略）：
```

結束時可撤銷 App 權限、移除 Keyala 中的 key；這不會在服務商端撤銷 key，需要時請到服務商官網撤銷。Settings → Account 可刪除帳號，需重新驗證且無法復原；只有確實想刪除時才執行。舊備份可能在到期前保留資料，詳見隱私頁。

## 日本語

### 始める前に

iOS 17 以降の iPhone とご自身の Apple Account をご用意ください。App Store で Apple の TestFlight を入れてから、Apple から届く Keyala の招待メールを開き、View in TestFlight → Accept → Install と進みます。デベロッパモードは不要です。Web での申込みだけでは招待は届きません。所要時間は約10〜15分。別の画面があれば QR を読み取れます。同じ iPhone ではリンクか KY- コードを使えます。

インストール後は TestFlight のバージョンと build を招待の記載と照合してください。アプリ内の Settings → Help & contact → App version でも確認できます。異なる場合は先に報告してください。一般のテスターはご自身の Apple Account を使い、Apple 審査専用の認証情報は使わないでください。

Keyala のベータは無料ですが、実際のモデル呼び出しにはご自身のプロバイダーの API 残高を使い、別途料金が発生します。ChatGPT や Claude のアプリのサブスクリプション自体に API 残高は含まれません。画面だけを見る場合は Settings → Developer options で開発者オプションを有効にし、Use sample data を選びます。オフラインのサンプルなので、ログイン、接続、通知、課金の確認には使えません。実際のテストではサンプルをオフにしてください。切替時はサインアウトします。サーバーの mock は偽のキーで連携を無料で試せますが、mock 設定の確認が必要です。サンプルのスイッチで本番サーバーが mock になるわけではありません。一般のテストでは本番のままオフラインサンプルを使うか、有料の実呼び出しを行うかを選んでください。Apple reviewer 用のログインは使いません。

### 最初のテスト

オフラインサンプルだけなら画面を確認して終了できます。以下の接続テストは任意で、本番ではプロバイダーの料金が発生する場合があります。

1. Keyala を開き、説明と言語を確認して Apple でサインインします。新規インストールは本番サーバーにつながります。サーバーの変更は不要です。
2. 実際の呼び出しを試す場合、Keys で OpenRouter に接続するか、ご自身の API キーを登録します。同意画面と料金を確認してください。利用上限は無料残高ではありません。
3. 招待で指定されたテスト用ツールを開きます。「架空の書店の面接で話す短い自己紹介を練習したい」など、架空の内容だけを入力して Keyala との接続を始めます。
4. Keyala で QR を読み取るか、iPhone で接続リンクを開きます。リンクでアプリが開かない場合は KY- コードを使います。アプリ名、サーバー、プロバイダー、請求先を確認し、最初は毎回確認する設定にします。
5. ツールから短いリクエストを1回送り、最初は Keyala を前面に置きます。Inbox で内容、モデル、見積り、OpenRouter の請求先を確認し、今回だけ許可します。ツールに応答が返り、Activity に記録があることを確認します。見積りや使用量からの計算額は請求書ではありません。
6. 次のリクエストは拒否し、ツールに拒否が表示されることを確認します。uncertain の場合は繰り返し送信せず、履歴を確認して request ID を非公開の窓口に報告してください。
7. Apps でツールを開き、Disconnect app を選んで確認します。送信済みのリクエストは完了する場合があります。次の利用には再接続が必要になることを確認します。言語を切り替え、再起動後も設定が残るかもお試しください。

失敗したら、その手順で止めて報告してください。再現のために追加料金を使う必要はありません。通知を許可してロック中の受信とタップ後の画面、端末認証のキャンセル、カメラ拒否後のコード入力も任意で試せます。通知が来なければアプリを開いて確認してください。前面での更新とプッシュ通知は別の経路です。

### 報告とテスト終了

TestFlight → Keyala → Send Beta Feedback を開き、Include Screenshot で機密情報を隠した画像を選ぶか、Don’t Include Screenshot で画像なしにします。下の報告内容を入力し Submit を押します。Apple にもフィードバックが届きます。または[サポート](https://keyala.io/support/?lang=ja)をご利用ください。バージョン／build、iPhone／iOS、言語、日時とタイムゾーン、手順、期待した結果と実際の結果を記載します。必要な場合だけ非公開の窓口に request ID を添えてください。API キー、パスワード、token、接続 QR／コード、非公開の会話、未加工のアカウント情報は送らないでください。申込みフォームは不具合の報告用ではありません。

```text
Keyala バージョン／build：
iPhone 機種／iOS バージョン／アプリの言語：
日時とタイムゾーン：
再現手順（順番に）：
期待した結果：
実際の結果：
再発したか（追加料金を使って再現する必要はありません）：
プロバイダー／モデル（必要な場合）：
request ID（必要な場合、非公開の窓口のみ）：
機密情報を隠したスクリーンショット（任意）：
```

終了時はアプリの権限を取り消し、Keyala 内のキーを削除できます。プロバイダー側のキーは失効しないため、必要なら公式コンソールでも取り消してください。Settings → Account のアカウント削除には再認証が必要で、元に戻せません。本当に削除する場合だけ実行してください。古いバックアップには期限までデータが残る場合があります。

## Links

- [User guide](https://keyala.io/guide/)
- [Privacy](https://keyala.io/privacy/)
- [Support](https://keyala.io/support/)
- [Apply for the beta](https://keyala.io/)


## 邀請 email 文面（寄送前填寫方括號）

這是 Joseph 的簡短說明信，不能取代 Apple 的 TestFlight 邀請。請填入實際可供收件者安裝的 build、已確認測試模式的工具網址，並隨信附上本指南相應語言段落或可存取的指南連結。沒有免費 mock 工具時，直接寫「離線樣本即可，真實呼叫為自選付費項目」，不要放 reviewer 入口。每位收件者分開寄送，避免公開測試者名單。

### English invitation (primary)

```text
Subject: Try Keyala on TestFlight

Hi [name],

I’d love your help trying Keyala, an iPhone app for reviewing and approving AI apps’ API requests. It takes about 10–15 minutes on iOS 17 or later.

Install Apple’s TestFlight first, then accept Apple’s separate email invitation and install Keyala. Please check that it shows [version (build)]. Use your own Apple Account to sign in.

Start with the attached quick guide. Test tool / mode: [URL and confirmed mock or paid mode; or offline samples only]. Try approving one request, denying another, and disconnecting the app if you choose the connected flow.

The beta and offline samples are free. Real provider calls use your own API credits and may cost money; they’re optional. Don’t use Apple reviewer credentials or send me keys, tokens or passwords. Please hide private content in screenshots.

Send issues through TestFlight → Keyala → Send Beta Feedback, with your build, iOS, iPhone model, steps, and expected versus actual result. Support: https://keyala.io/support/

Thanks,
Joseph
```

### 繁體中文邀請

```text
主旨：邀請你試用 Keyala TestFlight

嗨 [稱呼]，

想邀請你試用 Keyala，這是一個讓你在 iPhone 查看及批准 AI 工具 API 請求的 App。需要 iOS 17 以上，約 10–15 分鐘。

請先安裝 Apple TestFlight，再接受 Apple 另外寄來的邀請並安裝 Keyala，確認版本是 [版本（build）]，以自己的 Apple 帳號登入。

隨信附上短教學。測試工具／模式：[網址及已確認的 mock 或付費模式；或只用離線樣本]。若選連線測試，可試批准一次、拒絕一次，再撤銷工具權限。

測試版與離線樣本免費；真實服務商呼叫會使用你自己的 API 額度，可能產生費用，可自行選擇是否測試。請不要使用 Apple 審查專用帳密，也不要寄回 key、token 或密碼；截圖請先遮蔽私人內容。

遇到問題可從 TestFlight → Keyala → Send Beta Feedback 回報，附 build、iOS、機型、步驟、預期與實際結果。支援：https://keyala.io/support/?lang=zh-Hant

謝謝！
Joseph
```

### 日本語の招待

```text
件名：Keyala の TestFlight テストへのお誘い

[お名前] さん

iPhone で AI アプリの API リクエストを確認・承認できる Keyala を試していただけませんか。iOS 17 以降で、10〜15分ほどです。

Apple の TestFlight を入れ、別途 Apple から届く招待メールを受け取ったら Keyala をインストールしてください。[バージョン（build）] を確認し、ご自身の Apple Account でサインインします。

簡単な手順を添付しました。テスト用ツール／モード：[URL と確認済みの mock または有料モード、またはオフラインサンプルのみ]。接続テストを選ぶ場合は、1回の承認、拒否、アプリの接続解除をお試しください。

ベータとオフラインサンプルは無料です。実際のモデル呼び出しにはご自身の API 残高を使い、料金が発生する場合があるため任意です。Apple 審査用の認証情報は使わず、キー、token、パスワードは送らないでください。画像の非公開情報も隠してください。

不具合は TestFlight → Keyala → Send Beta Feedback から、build、iOS、機種、手順、期待した結果と実際の結果を添えてお知らせください。サポート：https://keyala.io/support/?lang=ja

よろしくお願いします。
Joseph
```

## Joseph 自測清單（手動填結果）

以下全部待親自執行，文件核對或模擬器成功不能代替真機驗收。每項記錄「通過／失敗／未測」、build、機型、iOS、時間；不同 build 的結果分開記錄。不要把實際帳密或收件者資料填進本文件或 Git。

- [ ] 核對 App Store Connect 的外部測試資格與群組可用 build；邀請文只寫收件者實際可安裝的版本。Ready to Submit 不代表已提交或通過 Beta Review。
- [ ] 在乾淨的外部測試裝置走 Apple email → 接受 → 安裝，確認 TestFlight 與 App 顯示同一 build。此流程由 Joseph 自行安排，不因本清單而自動寄邀請。
- [ ] 離線樣本：確認免費、無需真實 key；關閉樣本後重新登入自己的 Apple 帳號，確認正式服務首次登入及 EN／繁中／日文切換與重開保留。
- [ ] 使用已確認的私人 mock 環境與假 key 測批准 completed、拒絕 denied；另外記錄正式服務的真實 provider 流程是否未測。禁止把 mock 成功算作正式登入、計費或真實 provider 成功。
- [ ] 在 Apps → 工具 → Disconnect app 確認撤銷；再次請求應失去權限，重新配對後才能恢復。撤銷不會召回已送出的請求。
- [ ] 真機確認連接 QR／Universal Link；連結失敗時另測 KY- 代碼，分開記結果。確認取消裝置驗證不會批准。
- [ ] 真機鎖定／背景狀態測 APNs，點通知進正確請求；前景 Inbox 更新另記，不算推播通過。
- [ ] 測 TestFlight 回饋，附一張純虛構資料截圖與回報範本，確認送出；檢查沒有真實 key、token、私人 prompt、帳密或配對 QR／代碼。
- [ ] 僅在確實願意刪除的測試帳號驗收帳號刪除與 Apple 授權撤銷；失敗或未測就保留狀態，勿標通過。這是不可復原操作。

本清單不要求付費呼叫。若日後自行選擇真實 provider 測試，先確認費用與額度，再以虛構短文字做最少次數呼叫，單獨記錄結果；遇到 uncertain 不要重送。

安裝及回饋流程參考 [Apple TestFlight 使用說明](https://testflight.apple.com/)；按鈕顯示語言可能依裝置而異。App 操作標籤以實際安裝的 build 為準。
