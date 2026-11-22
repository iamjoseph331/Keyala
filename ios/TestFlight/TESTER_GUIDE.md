# Keyala beta tester guide

English | [繁體中文](#繁體中文) | [日本語](#日本語)

This guide accompanies a TestFlight invitation. Applying on the website does not install the app or guarantee an invitation. Use the build identified in your invitation; screenshots on the website may show sample data.

## English

### Before you start

- An iPhone running iOS 17 or later, an Apple Account, and Apple's TestFlight app.
- Your Keyala TestFlight invitation. Open it on the iPhone and install Keyala through TestFlight. Developer Mode and a computer are not required for TestFlight installation.
- About 10–15 minutes. A second screen is useful for scanning a connection QR; on the same phone, use the connection link or KY- code instead.
- A provider account with API credit only if you choose to test a real model call. Keyala beta access is free; provider usage is billed separately to your provider account. ChatGPT or Claude app subscriptions do not themselves supply API credit.

You can inspect the interface without a provider key: Settings → Developer options → enable developer options → Use sample data. Samples are offline and do not prove that sign-in, pairing, notifications, or billing works. Switch samples off before the real flow.

### Your first test

1. Open Keyala, read the introduction, choose a language, and sign in with Apple. Fresh installs use the production server automatically. Leave custom server settings unchanged for this test.
2. If testing real calls, connect one provider in Keys. Use Connect OpenRouter or paste your own provider key. Review the provider's consent and billing information. A spending limit is not free credit. Never send a key to us as feedback.
3. Open the test tool named in your invitation. Use fictional text, for example: “Help me practise a short interview introduction for a fictional bookstore job.” Start its Keyala connection.
4. Scan its connection QR in Keyala, or open its connection link on the phone. If the link does not open the app, use the KY- code. Check the app name, server, provider, and billing route before connecting. Keep the initial policy at Ask.
5. Send one short text request from that tool. Keep Keyala in the foreground for the first attempt. In Inbox, inspect the messages, model, estimated cost and any OpenRouter billing route; choose This call only. Confirm the tool receives a response and Activity shows the result. Estimates and usage-derived costs are not a provider invoice.
6. Send a second request and choose Deny. Confirm that the tool reports the denial. If a request is uncertain, do not repeatedly resubmit it: check history and report the request ID privately.
7. In Apps, revoke that tool's access. Confirm it requires a new connection for another call. Try changing the language and reopening Keyala; the preference should remain.

If a step fails, stop there and report it. You do not need to spend more to reproduce it. No provider key or credit is required to report a failure.

### Optional tests and stopping

- Allow notifications, lock the phone, and trigger one request. Record whether a notification arrives and whether tapping it opens the correct request. If none arrives, reopen Keyala; foreground refresh is a separate path.
- Cancel device verification once: approval should not be sent. Camera permission denial should still leave code entry available.
- Removing a key in Keyala does not revoke the key at the provider. To end access, revoke the app permission and remove the key; revoke the provider key at its official console if needed.
- Settings → Account offers account deletion with fresh verification. This is irreversible: use it only if you intend to delete the beta account, and do not recreate it merely for testing. Backups can retain older data until they expire; the privacy page explains handling.

### Feedback

Use TestFlight feedback or [Keyala support](https://keyala.io/support/). A draft email address is support@keyala.io. Include only what is needed:

```text
Keyala version and build (shown in TestFlight):
iPhone model / iOS version / app language:
Date, time and time zone:
Step where it failed:
What I expected / what happened:
Can it happen again? (No need to spend more to check.)
Provider and model, if relevant:
Request ID, if relevant (private support only):
Screenshot with sensitive information hidden, optional:
```

Never include API keys, passwords, session/grant tokens, pairing QR/code, private prompts/responses, or unredacted account details. Check screenshots before sending. The signup form is for applications, not bug reports.

## 繁體中文

### 開始前

準備 iOS 17 以上的 iPhone、Apple 帳號、TestFlight App，以及收到的 Keyala 測試邀請。從邀請進入 TestFlight 安裝，不需要開啟開發者模式。網站報名不等於已收到邀請。預留 10–15 分鐘；另一個螢幕方便掃 QR，同一支手機可改用連結或 KY- 代碼。

Keyala 測試版免費；真實模型呼叫會使用你自己的服務商 API 額度，另外計費。ChatGPT／Claude App 訂閱本身不等於 API 額度。只想看介面，可到 Settings → Developer options 啟用開發者選項，再開 Use sample data；這是離線範例，不能驗證登入、配對、推播或計費。實測前請關閉範例資料。

### 首次測試

1. 開啟 Keyala、閱讀說明、選擇語言，以 Apple 登入。全新安裝預設連正式服務，不需修改伺服器。
2. 若要測真實呼叫，在 Keys 連接一個服務商：可連 OpenRouter 或貼上自己的 API key。先確認同意畫面、付款與額度；支出上限不是免費額度。
3. 開啟邀請中指定的測試工具。只填虛構資料，例如「請幫我練習應徵虛構書店的一分鐘自我介紹」，開始 Keyala 連接。
4. 用 Keyala 掃工具的 QR，或在手機點連結；連結無法開 App 時可輸入 KY- 代碼。確認 App、伺服器、服務商及計費路徑，初始權限維持逐次詢問。
5. 從工具送出一個短文字請求，第一次先讓 Keyala 保持前景。在 Inbox 檢查訊息、模型、預估費用及 OpenRouter 計費資訊，只批准這一次。確認工具收到回應，Activity 有紀錄。預估與依用量計算的費用不是服務商帳單。
6. 第二次選拒絕，確認工具顯示被拒絕。遇到 uncertain 不要連續重送，先看歷史並私下回報 request ID。
7. 到 Apps 撤銷工具權限，確認再次呼叫需要重新連接。切換語言、重開 App，確認語言保留。

失敗就停在該步回報，不必為了重現問題繼續花費。也可測通知、鎖定畫面點通知、取消裝置驗證，以及拒絕相機權限後輸入代碼。沒有通知時可先開 App 查看，前景更新與推播是不同路徑。

### 回報與結束測試

透過 TestFlight 回饋或 [支援頁](https://keyala.io/support/?lang=zh-Hant) 回報。附版本／build、iPhone／iOS、語言、時間與時區、操作步驟、預期與實際結果；需要時私下附 request ID。不要附 API key、密碼、token、配對 QR／代碼、私人對話或未遮蔽帳號資料。報名表不用來回報 bug。

結束時可撤銷 App 權限、移除 Keyala 中的 key；這不會在服務商端撤銷 key，需要時請到服務商官網撤銷。Settings → Account 可刪除帳號，需重新驗證且無法復原；只有確實想刪除時才執行。舊備份可能在到期前保留資料，詳見隱私頁。

## 日本語

### 始める前に

iOS 17 以降の iPhone、Apple Account、TestFlight アプリ、Keyala のテスト招待をご用意ください。招待から TestFlight でインストールします。デベロッパモードは不要です。Web での申込みだけでは招待は届きません。所要時間は約10〜15分。別の画面があれば QR を読み取れます。同じ iPhone ではリンクか KY- コードを使えます。

Keyala のベータは無料ですが、実際のモデル呼び出しにはご自身のプロバイダーの API 残高を使い、別途料金が発生します。ChatGPT や Claude のアプリのサブスクリプション自体に API 残高は含まれません。画面だけを見る場合は Settings → Developer options で開発者オプションを有効にし、Use sample data を選びます。オフラインのサンプルなので、ログイン、接続、通知、課金の確認には使えません。実際のテストではサンプルをオフにしてください。

### 最初のテスト

1. Keyala を開き、説明と言語を確認して Apple でサインインします。新規インストールは本番サーバーにつながります。サーバーの変更は不要です。
2. 実際の呼び出しを試す場合、Keys で OpenRouter に接続するか、ご自身の API キーを登録します。同意画面と料金を確認してください。利用上限は無料残高ではありません。
3. 招待で指定されたテスト用ツールを開きます。「架空の書店の面接で話す短い自己紹介を練習したい」など、架空の内容だけを入力して Keyala との接続を始めます。
4. Keyala で QR を読み取るか、iPhone で接続リンクを開きます。リンクでアプリが開かない場合は KY- コードを使います。アプリ名、サーバー、プロバイダー、請求先を確認し、最初は毎回確認する設定にします。
5. ツールから短いリクエストを1回送り、最初は Keyala を前面に置きます。Inbox で内容、モデル、見積り、OpenRouter の請求先を確認し、今回だけ許可します。ツールに応答が返り、Activity に記録があることを確認します。見積りや使用量からの計算額は請求書ではありません。
6. 次のリクエストは拒否し、ツールに拒否が表示されることを確認します。uncertain の場合は繰り返し送信せず、履歴を確認して request ID を非公開の窓口に報告してください。
7. Apps でツールの権限を取り消し、次の利用には再接続が必要になることを確認します。言語を切り替え、再起動後も設定が残るかもお試しください。

失敗したら、その手順で止めて報告してください。再現のために追加料金を使う必要はありません。通知を許可してロック中の受信とタップ後の画面、端末認証のキャンセル、カメラ拒否後のコード入力も任意で試せます。通知が来なければアプリを開いて確認してください。前面での更新とプッシュ通知は別の経路です。

### 報告とテスト終了

TestFlight のフィードバックか[サポート](https://keyala.io/support/?lang=ja)をご利用ください。バージョン／build、iPhone／iOS、言語、日時とタイムゾーン、手順、期待した結果と実際の結果を記載します。必要な場合だけ非公開の窓口に request ID を添えてください。API キー、パスワード、token、接続 QR／コード、非公開の会話、未加工のアカウント情報は送らないでください。申込みフォームは不具合の報告用ではありません。

終了時はアプリの権限を取り消し、Keyala 内のキーを削除できます。プロバイダー側のキーは失効しないため、必要なら公式コンソールでも取り消してください。Settings → Account のアカウント削除には再認証が必要で、元に戻せません。本当に削除する場合だけ実行してください。古いバックアップには期限までデータが残る場合があります。

## Links

- [User guide](https://keyala.io/guide/)
- [Privacy](https://keyala.io/privacy/)
- [Support](https://keyala.io/support/)
- [Apply for the beta](https://keyala.io/)
