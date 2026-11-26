# Keyala TestFlight tester guide

The public guides contain the full pilot tasks and the feedback form for each language.

- [English](https://keyala.io/testflight/)
- [繁體中文](https://keyala.io/zh-Hant/testflight/)
- [日本語](https://keyala.io/ja/testflight/)

## English

Thank you for helping test Keyala. Try as much as you have time for and let us know where you got stuck. The invite-only pilot runs for 14 days with a minimum of 10 and a maximum of 20 testers. The invitation will confirm the dates and build.

Install [Apple TestFlight](https://apps.apple.com/app/testflight/id899247664) on an iPhone with iOS 17 or later. Open Apple’s invitation and select **View in TestFlight → Accept → Install**. Compare the build to the invitation. Sign in with your own Apple or Google account. They are separate Keyala accounts even with the same email. Keep using the same account and sign-in method.

After seeing Keyala’s first screens, write one sentence about what you think it does before opening any explanations in the guide. Then try adding the pilot key, connecting Interview Prepper, allowing one call, denying another, checking Activity and disconnecting. A later attempt should require a new connection.

Joseph will send each tester a private OpenRouter key limited to US$1 for the free connected test. The US$1 is a total limit with no reset or automatic top-up. It expires at the shared 14-day pilot end shown in your invitation. Cost per call varies, and there is no automatic paid fallback at exhaustion. Use **Keys → Paste a key on this iPhone**, select **OpenRouter** and save. You can paste the key without an OpenRouter account or OAuth. Keep any existing OpenRouter key and contact Joseph before adding a pilot key. Your own key uses your provider balance and can incur charges. The supplied key is separate from the future coupon concept, which has no test flow, announced pricing or release date.

For the sponsored or your own OpenRouter key, turn on **Settings → Use OpenRouter for OpenAI and Anthropic** before pairing. To use your own Anthropic key directly, leave the switch off and check that your Anthropic key is saved. Disconnect and reconnect to change a route already in use. In [Interview Prepper](https://iamjoseph331.github.io/interview_prepper/), choose **Connect Keyala** and use the link, QR code or KY- code. Keep **Connect and ask each time**. Set **Junior**, **Machine learning fundamentals** and **1** question. Open **Graded session → Configure** and select **Haiku 5.5** (the list shown after connecting uses `claude-haiku-5-5`). Start the session for a fictional junior AI/ML interview. A session can send several requests. Read each one in Inbox, approve only **This call only**, and use a follow-up for the denial task. In **Inbox → Activity**, check results. Disconnect in **Apps** and check that Interview Prepper asks to reconnect. Stop on an error or uncertain state and report it without spending more to reproduce it.

Keyala stores the key encrypted on its server. Connected apps get permission and approved results. They never get the key. This relies on trust in the Keyala server operator, and the pilot sponsor may see provider usage or logs. Use fictional inputs. Offline browsing is available under **Settings → Developer options → Use sample data**. Switching signs you out. Offline samples make no network calls and provide no evidence about the live integration. Notifications and language persistence are optional. Account deletion is irreversible and outside the pilot tasks.

### Optional example apps

Interview Prepper is the main exercise. If you want to try another app, keep Keyala set to Ask for each call. Paste API keys only into Keyala. Use fictional inputs. You can repeat the approval, denial, Activity and revocation checks with either app.

**[Flashcards](https://keyala.io/examples/flashcards/)**

To try the review without a key or model call, open **Example deck** under **My decks**. Flip a card and choose **Know** or **Don’t know**. Generated decks and their scores stay in this browser.

To generate one deck with `gpt-4o-mini`, choose a language, level and topic (for example, beginner Japanese for ordering in a fictional café), set **Cards** to **10** and choose **Make deck**. If prompted, connect with Keyala. Use an OpenAI key directly with the routing switch off. For the sponsored or your own OpenRouter key, turn on **Settings → Use OpenRouter for OpenAI and Anthropic** before connecting. To change the route for an existing Flashcards connection, revoke it in Keyala and reconnect. Check the route and estimated cost, then approve only the deck request.

**[Daily Reflection](https://keyala.io/examples/daily-reflection/)**

Write one or two fictional lines, such as “I sketched a paper boat in an imaginary art class.” Choose **Reflect**, connect with Keyala if prompted and approve one request. Follow-ups use **Send**. The app calls `anthropic/claude-haiku-5.5` directly through OpenRouter with your saved OpenRouter key. The OpenAI/Anthropic routing switch is irrelevant to this connection.

Each message approved and sent to the model is billable and resends that day’s conversation. The daily limit is 10 of your messages. Keep Ask even if the page suggests **Next calls** or **Budget**. The journal is saved in this browser and does not sync. Use only invented entries, with no real diary text or sensitive information.

The [English feedback form](https://docs.google.com/forms/d/e/1FAIpQLSdqT46s_9CAEkFoq_1YImjQ2rrBOMtjLoo-Ora2qWRq18n_NQ/viewform) is optional and needs no sign-in. You can send feedback even if you could not install Keyala.

For bugs, reply to the introductory email or email support@keyala.io. **TestFlight → Keyala → Send Beta Feedback** also sends the report to Apple. Optional context includes build, iPhone model, iOS version, language, date/time zone, steps, expected and actual result, provider/model and a request ID sent privately to support. Redact screenshots and omit keys, credentials, tokens, pairing codes/QRs, private inputs and responses, and personal details.

## 繁體中文

謝謝你試用 Keyala！有空做到哪裡就好。測試採邀請制，為期 14 天，最少 10 人、最多 20 人。日期與測試版本會在邀請中確認。

在 iOS 17 以上的 iPhone 安裝 [Apple TestFlight](https://apps.apple.com/app/testflight/id899247664)。開啟 Apple 寄來的邀請，在 TestFlight 接受並安裝 Keyala，確認版本與組建編號。用自己的 Apple 或 Google 帳號登入。即使電子郵件相同，也會建立各自獨立的 Keyala 帳號。測試期間請沿用同一種登入方式及同一個帳號。

閱讀指南說明前，請先看 App 的開場畫面，再用自己的話寫一句 Keyala 的用途。接著試著新增測試金鑰、連結 Interview Prepper、批准一次、拒絕一次、查看活動紀錄、撤銷連結，並確認下次使用需要重新連結。

Joseph 會私下提供每人一把支出上限為 1 美元的 OpenRouter 測試金鑰。1 美元是整段測試的總上限，額度不會重設或自動補充。金鑰在邀請中指定的共同 14 天測試結束時到期。呼叫次數取決於用量，額度用完不會自動改用其他付費金鑰。在 **Keys → 在這台 iPhone 貼上 key** 選 OpenRouter 並儲存。不需要 OpenRouter 帳號或 OAuth。已有自己的 OpenRouter 金鑰時，請保留原金鑰，先聯絡 Joseph。自帶金鑰會使用你的服務商餘額，可能產生費用。測試金鑰由專案方手動提供。分享優惠券尚未實作，也未公布價格和推出日期。

使用測試金鑰或自己的 OpenRouter 金鑰時，配對前在設定開啟**透過 OpenRouter 使用 OpenAI 與 Anthropic**。要直接使用自帶的 Anthropic 金鑰，請關閉開關並確認已儲存金鑰。要變更既有連結的計費路徑，請先中斷再連結。在 [Interview Prepper](https://iamjoseph331.github.io/interview_prepper/) 選 **Connect Keyala**，使用連結、QR 碼或 KY- 代碼，維持**連結並每次詢問**。選 **Junior** 與 **Machine learning fundamentals**，設為 **1** 題。在 **Graded session → Configure** 選 **Haiku 5.5**（連結後的清單使用 `claude-haiku-5-5`），開始虛構的初階 AI／機器學習面試。一次練習可能觸發多筆呼叫，請在收件匣逐筆檢查並只批准一次，再拒絕下一筆。用**收件匣 → 活動紀錄**查看結果，並從 **Apps** 中斷連結。失敗或顯示結果不確定時，先停下來回報，無須再花額度重現。

Keyala 將金鑰加密存放於伺服器，連結工具只會取得授權及批准後的回應。使用時需要信任伺服器營運者，測試贊助者也可能看得到服務商的用量或紀錄。請使用虛構資料。只想看畫面可用**設定 → 開發者選項 → 使用範例資料**。切換會登出，離線範例不會連網，無法驗證實際整合。通知及重開後語言保留可自行加測。帳號刪除無法復原，本次不用測。

### 選做的範例 App

Interview Prepper 是主要測試。若想再試其他 App，請維持逐次詢問（Ask），並只在 Keyala 貼上 API 金鑰。請使用虛構資料。兩個 App 都可以用來再試一次批准、拒絕、查看活動紀錄及撤銷授權。

**[Flashcards 單字卡](https://keyala.io/examples/flashcards/)**

到**我的卡組**開啟**範例卡組**，即可翻牌，並選**會**或**不會**，不用金鑰，也不會呼叫模型。產生的卡組和複習分數都保存在這個瀏覽器。

要用 `gpt-4o-mini` 產生一副卡組，選語言、程度和主題，例如初級日文及在虛構咖啡店點餐。將**張數**設為 **10**，點**製作卡組**，依提示連結 Keyala。直接使用自帶 OpenAI 金鑰時保持路由開關關閉。若使用測試金鑰或自己的 OpenRouter 金鑰，請在配對前到**設定**開啟**透過 OpenRouter 使用 OpenAI 與 Anthropic**。要變更 Flashcards 既有連結的路由，請先在 Keyala 撤銷授權再重新連結。確認計費路徑和預估費用，再只批准這一副卡組的請求。

**[Daily Reflection 每日回顧](https://keyala.io/examples/daily-reflection/)**

輸入一兩句虛構內容，例如「今天在虛構的美術課上畫了一艘紙船。」點**回顧今天**，依提示連結 Keyala，並只批准這一筆請求。後續回覆使用**送出**。App 會用已儲存的 OpenRouter 金鑰直接透過 OpenRouter 呼叫 `anthropic/claude-haiku-5.5`，不需要調整 OpenAI／Anthropic 路由開關。

每則批准並送往模型的訊息都會計費，並重送當天的對話。每天最多可送出 10 則訊息。即使頁面提示使用**接下來幾次**或**預算**，請維持逐次詢問。日記只存在這個瀏覽器，不會同步。請使用虛構內容，勿輸入真實日記或敏感資料。

[繁體中文回饋表](https://docs.google.com/forms/d/e/1FAIpQLSeym6NgP_VFPEh_fdAvUWnTndogjh-Jy6ngaDjWQkb574BRlA/viewform) 可自由填寫，無須登入。即使沒能安裝 Keyala，也歡迎填寫。

有問題請回覆介紹信或寄到 support@keyala.io。透過 **TestFlight → Keyala → 傳送 Beta 版回饋**送出的內容也會交給 Apple。可附版本與組建編號、iPhone／iOS、語言、日期時間與時區、步驟、預期及實際結果、服務商／模型，或私下提供請求 ID。請先遮蔽截圖中的敏感內容，並省略金鑰、帳密、權杖、配對碼及 QR 碼、私人對話和個資。

## 日本語

Keyala を試していただき、ありがとうございます。時間のある範囲でお試しください。今回は招待制で、期間は14日間、参加人数は10〜20人です。日程と対象ビルドは招待時にお知らせします。

iOS 17 以降の iPhone に [Apple の TestFlight](https://apps.apple.com/app/testflight/id899247664) を入れます。Apple から届く Keyala の招待を開き、TestFlight で承認してインストールします。ビルド番号を照合し、ご自身の Apple Account または Google アカウントでサインインしてください。メールアドレスが同じでも別々の Keyala アカウントになります。テスト中は同じ方法とアカウントを使ってください。

説明を読む前に、最初の画面を見て Keyala の用途をご自身の言葉で一文書いてください。その後、キーの追加、Interview Prepper への接続、1回だけの許可、別のリクエストの拒否、履歴確認、接続の解除をお試しください。解除後は、次の利用に再接続が必要になることを確認します。

Joseph が利用上限1米ドルの OpenRouter キーを一人ずつお渡しします。1米ドルは期間全体の上限です。リセットや自動追加はありません。招待に記載した共通の14日間が終わると失効します。呼び出し回数は使用量によって変わり、使い切った後に別の有料キーへ自動で切り替わることはありません。**キー → この iPhone にキーを貼り付け**で OpenRouter を選び、保存します。OpenRouter のアカウントや OAuth は不要です。登録済みの OpenRouter キーはそのまま残し、追加する前に Joseph にご連絡ください。ご自身のキーを使う場合は、プロバイダーの残高を使い、料金が発生することがあります。今回のキーは個別にお渡しするものです。クーポン共有は未実装で、価格と提供開始日は未発表です。

今回お渡しするキーやご自身の OpenRouter キーを使う場合は、接続前に設定で **OpenAI と Anthropic に OpenRouter を使う**をオンにします。ご自身の Anthropic キーを直接使う場合はオフにし、キーが登録済みであることを確認します。既存の請求先を変える場合は解除してつなぎ直します。[Interview Prepper](https://iamjoseph331.github.io/interview_prepper/) で **Connect Keyala** を選び、リンク、QR コード、または KY- コードでつなぎ、**接続して毎回確認**を選びます。**Junior** と **Machine learning fundamentals** を選び、質問は **1** 件にします。**Graded session → Configure** で **Haiku 5.5**（接続後の一覧では `claude-haiku-5-5`）を選び、架空の若手 AI・機械学習エンジニアの面接を始めます。複数のリクエストが発生する場合は1件ずつ確認し、1回だけ許可した後で次のリクエストを拒否します。**受信トレイ → 履歴**で結果を確認し、**アプリ**から接続を解除してください。失敗や結果不明の場合は追加で送信せず、そこで止めてお知らせください。

Keyala はキーを暗号化してサーバーに保存します。接続したアプリが受け取るのは権限と許可済みの結果です。利用には Keyala のサーバー運営者を信頼することが必要です。テスト費用の負担者がプロバイダーの利用状況やログを見られる場合があります。入力は架空の内容にしてください。画面だけ見る場合は、**設定 → 開発者向け設定 → サンプルデータを使用**を選びます。切替時はサインアウトします。オフラインでは通信しないため、実際の接続や課金は確認できません。通知や言語設定の保存は任意です。元に戻せないアカウント削除は今回の対象外です。

### 任意で試せるサンプルアプリ

主なテストは Interview Prepper です。別のアプリを試す場合も Keyala は毎回確認（Ask）のままにしてください。API キーを貼り付ける先は Keyala だけにし、入力には架空の内容を使います。どちらのアプリでも、許可、拒否、履歴の確認、接続の解除を改めて試せます。

**[Flashcards 単語カード](https://keyala.io/examples/flashcards/)**

**マイデッキ**で**サンプルデッキ**を開きます。カードをめくり、**わかる**か**わからない**を選びます。サンプルを使うだけならキーは不要で、モデルの呼び出しもありません。生成したデッキや復習の結果はこのブラウザに保存されます。

`gpt-4o-mini` でデッキを1つ作るには、学ぶ言語、レベル、トピックを選びます。例えば初級日本語で、架空のカフェでの注文を指定します。**枚数**を **10** にして**デッキを作る**を押し、案内に従って Keyala に接続します。ご自身の OpenAI キーを直接使う場合はルーティングのスイッチをオフにします。今回お渡しするキーやご自身の OpenRouter キーを使う場合は、接続前に**設定**で **OpenAI と Anthropic に OpenRouter を使う**をオンにします。既存の Flashcards 接続の請求先を変える場合は、Keyala で解除してからつなぎ直します。請求先と概算費用を確認し、そのデッキのリクエストだけを許可します。

**[Daily Reflection 一日の振り返り](https://keyala.io/examples/daily-reflection/)**

「架空の美術の授業で紙の船を描いた」など、架空の出来事を1〜2行書きます。**振り返る**を押し、案内が出たら Keyala に接続してリクエストを1件だけ許可します。続きは**送信**を使います。登録済みの OpenRouter キーで `anthropic/claude-haiku-5.5` を直接呼び出すため、OpenAI や Anthropic 向けのルーティング設定は不要です。

許可してモデルに送信したメッセージには毎回料金がかかり、その日の会話も再送されます。送れるメッセージは1日10件までです。**次の数回**や**予算**が提案されても、毎回確認を続けてください。日記はこのブラウザに保存され、同期されません。本当の日記や機密情報は入力せず、架空の内容を使ってください。

[日本語のフィードバックフォーム](https://docs.google.com/forms/d/e/1FAIpQLScOWcYcknRf1R76zxO-cL6VLR_2RQpekE-PN176NG2D8kYlmg/viewform)への回答は任意で、ログインは不要です。Keyala をインストールできなかった場合も送信できます。

不具合は案内メールへの返信か support@keyala.io へお願いします。**TestFlight → Keyala → ベータ版フィードバックを送信**で送った内容は Apple にも届きます。必要に応じてビルド番号、iPhone と iOS、言語、日時とタイムゾーン、手順、期待した結果と実際の結果、プロバイダーとモデルを添えてください。必要ならリクエスト ID をサポートに個別に送れます。画像の機密情報は隠し、キー、認証情報、トークン、接続コードや QR コード、個人情報、非公開の入力や応答は送らないでください。

## 邀請 email 文面

先填上實際測試期間及收件者可安裝的版本與組建編號。這封介紹信搭配 Apple 的正式 TestFlight 邀請。各收件者分開寄送，金鑰也個別提供。

### English

```text
Subject: Try Keyala on TestFlight

Hi [name],

Would you like to help test Keyala? The pilot runs from [start date] to [end date]. You’ll need an iPhone with iOS 17 or later and Apple’s TestFlight app. Please use Apple’s separate invitation to install build [build].

Here’s the guide: https://keyala.io/testflight/

I’ll send a private OpenRouter key with a US$1 limit for the free connected test. Try as much as you have time for. If any step gets confusing, please reply and tell me where you stopped.

Thanks,
Joseph
```

### 繁體中文

```text
主旨：邀請你試用 Keyala TestFlight

嗨 [稱呼]，

想邀請你試用 Keyala，測試期間為 [開始日] 至 [結束日]。需要一支 iOS 17 以上的 iPhone 與 Apple TestFlight。請透過 Apple 另外寄來的邀請安裝 [版本與組建編號]。

測試指南：https://keyala.io/zh-Hant/testflight/

我會私下提供一把上限 1 美元的 OpenRouter 金鑰，讓你免費測連線流程。有空做到哪裡就好。若卡住，直接回覆這封信告訴我停在哪一步即可。

謝謝！
Joseph
```

### 日本語

```text
件名：Keyala の TestFlight テストへのお誘い

[お名前] さん

[開始日] から [終了日] まで、Keyala を試していただけませんか。iOS 17 以降の iPhone と Apple の TestFlight が必要です。Apple から別途届く招待で、[バージョンとビルド番号] をインストールしてください。

テストガイド：https://keyala.io/ja/testflight/

無料で接続テストができるよう、利用上限1米ドルの OpenRouter キーを個別にお渡しします。時間のある範囲でお試しください。途中で困ったら、どの手順かをこのメールへの返信で教えていただけると助かります。

よろしくお願いします。
Joseph
```

## Joseph 自測清單（手動填結果）

以下全部待親自執行，文件核對或模擬器成功不能代替真機驗收。每項記錄「通過／失敗／未測」、build、機型、iOS、時間；不同 build 的結果分開記錄。不要把實際帳密或收件者資料填進本文件或 Git。

- [ ] 核對 App Store Connect 的外部測試資格與群組可用 build；邀請文只寫收件者實際可安裝的版本。Ready to Submit 不代表已提交或通過 Beta Review。
- [ ] 在乾淨的外部測試裝置走 Apple email → 接受 → 安裝，確認 TestFlight 與 App 顯示同一 build。此流程由 Joseph 自行安排，不因本清單而自動寄邀請。
- [ ] 離線樣本：確認免費、無需真實 key；關閉樣本後用原本的 Apple 或 Google 帳號重新登入，確認正式服務首次登入及 EN／繁中／日文切換與重開保留。
- [ ] 使用已確認的私人 mock 環境與假 key 測批准 completed、拒絕 denied；另外記錄正式服務的真實 provider 流程是否未測。Apple 審查專用環境不向一般測試者開放。禁止把 mock 成功算作正式登入、計費或真實 provider 成功。
- [ ] 在 Apps → 工具 → Disconnect app 確認撤銷；再次請求應失去權限，重新配對後才能恢復。撤銷不會召回已送出的請求。
- [ ] 真機確認連接 QR／Universal Link；連結失敗時另測 KY- 代碼，分開記結果。確認取消裝置驗證不會批准。
- [ ] 真機鎖定／背景狀態測 APNs，點通知進正確請求；前景 Inbox 更新另記，不算推播通過。
- [ ] 測 TestFlight 回饋，附一張純虛構資料截圖與回報範本，確認送出；檢查沒有真實 key、token、私人 prompt、帳密或配對 QR／代碼。
- [ ] 僅在確實願意刪除的測試帳號驗收帳號刪除與 Apple 授權撤銷；失敗或未測就保留狀態，勿標通過。這是不可復原操作。

本清單不要求付費呼叫。若要驗收正式服務的贊助路徑，使用測試專用 OpenRouter 金鑰，確認金鑰上限、路由與回饋表可用性。先以虛構文字做最少次數的呼叫，單獨記錄結果。遇到 uncertain 就停止並回報。

安裝及回饋流程參考 [Apple TestFlight 使用說明](https://testflight.apple.com/)；按鈕顯示語言可能依裝置而異。App 操作標籤以實際安裝的 build 為準。
