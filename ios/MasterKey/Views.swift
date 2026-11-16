import SwiftUI
import AuthenticationServices

struct PageHeader: View {
    let eyebrow: String
    let title: String
    let subtitle: String
    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(eyebrow.uppercased()).font(.caption.weight(.bold)).tracking(1.6).foregroundStyle(MKStyle.teal)
            Text(title).font(.system(size: 34, weight: .semibold, design: .rounded)).foregroundStyle(MKStyle.cream)
            Text(subtitle).font(.subheadline).foregroundStyle(MKStyle.muted)
        }.frame(maxWidth: .infinity, alignment: .leading).padding(.bottom, 10)
    }
}

struct Card<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View { content.padding(16).frame(maxWidth: .infinity, alignment: .leading).background(MKStyle.panel, in: RoundedRectangle(cornerRadius: 20, style: .continuous)).overlay(RoundedRectangle(cornerRadius: 20).stroke(.white.opacity(0.06), lineWidth: 1)) }
}

struct InfoBanner: View {
    @EnvironmentObject private var store: AppStore
    var body: some View {
        if let message = store.infoMessage {
            Button { store.infoMessage = nil } label: {
                HStack(alignment: .top, spacing: 10) { Image(systemName: "info.circle.fill").foregroundStyle(MKStyle.teal); Text(message).font(.footnote).foregroundStyle(MKStyle.cream); Spacer() }.padding(12).background(MKStyle.teal.opacity(0.12), in: RoundedRectangle(cornerRadius: 14))
            }.buttonStyle(.plain)
        }
    }
}

struct InboxView: View {
    @EnvironmentObject private var store: AppStore
    @State private var showHistory = false
    @State private var selected: RequestRecord?
    private var visible: [RequestRecord] { showHistory ? store.requests.filter { $0.status != "pending" } : store.requests.filter { $0.status == "pending" } }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 18) {
                PageHeader(eyebrow: "Your keys. Your call.", title: "Requests", subtitle: "Review pending calls and revisit past requests and responses.")
                ModeBanner()
                InfoBanner()
                Picker("Request list", selection: $showHistory) {
                    Text("Pending").tag(false)
                    Text("History").tag(true)
                }
                .pickerStyle(.segmented)
                if visible.isEmpty {
                    Card { VStack(alignment: .leading, spacing: 10) { Image(systemName: "checkmark.circle").font(.title2).foregroundStyle(MKStyle.teal); Text(showHistory ? (store.requests.isEmpty ? "No request history yet" : "No decided requests in this part of history") : "All clear").font(.headline); Text(showHistory ? "Older requests will load below when available." : "New requests appear here for your review.").font(.subheadline).foregroundStyle(MKStyle.muted) } }
                }
                ForEach(visible) { request in Button { selected = request } label: { RequestCard(request: request) }.buttonStyle(.plain) }
                if showHistory && !store.demoMode {
                    if let error = store.olderRequestsError {
                        Text(error).font(.footnote).foregroundStyle(.orange).frame(maxWidth: .infinity, alignment: .leading)
                    }
                    if store.isLoadingOlderRequests {
                        ProgressView("Loading older requests…").frame(maxWidth: .infinity).tint(MKStyle.teal)
                    } else if store.hasOlderRequests {
                        Button { Task { await store.loadOlderRequests() } } label: {
                            Label(store.olderRequestsError == nil ? "Load older requests" : "Try again", systemImage: store.olderRequestsError == nil ? "arrow.down" : "arrow.clockwise")
                                .frame(maxWidth: .infinity).padding(14).background(.white.opacity(0.06), in: Capsule())
                        }
                    } else {
                        Text("You’ve reached the beginning of your request history.").font(.footnote).foregroundStyle(MKStyle.muted).frame(maxWidth: .infinity)
                    }
                }
                Button { Task { await store.refresh() } } label: { Label("Refresh requests", systemImage: "arrow.clockwise").frame(maxWidth: .infinity).padding(14).background(.white.opacity(0.06), in: Capsule()) }
            }.padding(.horizontal, 20).padding(.top, 18).padding(.bottom, 30)
        }.background(MKStyle.navy.ignoresSafeArea()).navigationBarHidden(true)
            .sheet(item: $selected) { RequestDetailView(request: request(for: $0)) }
            .onChange(of: store.pendingRequestID) { _, _ in openNotificationRequest() }
            .onChange(of: store.requests.map(\.id)) { _, _ in openNotificationRequest() }
            .onChange(of: store.contextGeneration) { _, _ in selected = nil }
            .onAppear { openNotificationRequest() }
    }
    private func openNotificationRequest() {
        guard let id = store.pendingRequestID, let item = store.requests.first(where: { $0.id == id }) else { return }
        showHistory = item.status != "pending"
        selected = item
        store.pendingRequestID = nil
    }
    private func request(for item: RequestRecord) -> RequestRecord { store.requests.first(where: { $0.id == item.id }) ?? item }
}

struct ModeBanner: View {
    @EnvironmentObject private var store: AppStore
    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: store.demoMode ? "sparkles.rectangle.stack" : "lock.shield.fill").foregroundStyle(MKStyle.teal)
            VStack(alignment: .leading, spacing: 2) {
                Text(store.demoMode ? "DEMO WORKSPACE" : (store.connected ? "CONNECTED" : "LIVE SERVER" )).font(.caption.weight(.heavy)).tracking(1)
                Text(store.demoMode ? "Sample data only. No key or provider call is real." : (store.connected ? store.serverURL : "Sign in to load your data.")).font(.caption).foregroundStyle(MKStyle.muted).lineLimit(2)
            }
            Spacer(minLength: 0)
            Circle().fill(store.demoMode ? Color.orange : (store.connected ? MKStyle.teal : Color.gray)).frame(width: 8, height: 8)
        }.padding(14).background(MKStyle.panel, in: RoundedRectangle(cornerRadius: 16))
    }
}

struct RequestCard: View {
    let request: RequestRecord
    var body: some View {
        Card {
            VStack(alignment: .leading, spacing: 12) {
                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 4) { Text(request.appName).font(.headline).foregroundStyle(MKStyle.cream); Text("\(request.provider.title)  /  \(request.model)").font(.caption).foregroundStyle(MKStyle.muted) }
                    Spacer(); StatusPill(status: request.status)
                }
                Text(preview).font(.subheadline).foregroundStyle(MKStyle.cream.opacity(0.86)).lineLimit(2).multilineTextAlignment(.leading)
                HStack { Label(request.estimatedUSD, systemImage: "chart.line.uptrend.xyaxis"); Spacer(); Text(request.status == "pending" ? "Review request  ›" : request.createdAt.asDate).foregroundStyle(MKStyle.muted) }.font(.caption.weight(.medium)).foregroundStyle(MKStyle.teal)
            }
        }
    }
    private var preview: String {
        guard case .object(let object)? = request.payload else { return "Open to inspect the complete request payload." }
        if case .array(let messages)? = object["messages"], let first = messages.last,
           case .object(let message) = first, case .string(let content)? = message["content"] { return content }
        return "Open to inspect the complete request payload."
    }
}

struct StatusPill: View {
    let status: String
    var color: Color { status == "pending" ? .orange : (status == "completed" || status == "approved" ? MKStyle.teal : MKStyle.muted) }
    var body: some View { Text(status.uppercased()).font(.system(size: 9, weight: .heavy, design: .rounded)).tracking(0.7).foregroundStyle(color).padding(.horizontal, 9).padding(.vertical, 6).background(color.opacity(0.12), in: Capsule()) }
}

struct RequestDetailView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    @State private var showPolicy = false
    @State private var loadingDetail = false
    let request: RequestRecord
    private var current: RequestRecord { store.requests.first(where: { $0.id == request.id }) ?? request }
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    HStack { VStack(alignment: .leading, spacing: 4) { Text(current.appName).font(.title2.bold()); Text("\(current.provider.title)  /  \(current.model)").foregroundStyle(MKStyle.muted) }; Spacer(); StatusPill(status: current.status) }
                    Card {
                        VStack(spacing: 12) {
                            HStack { Text("Estimated reservation").foregroundStyle(MKStyle.muted); Spacer(); Text(current.estimatedUSD).fontWeight(.bold).foregroundStyle(MKStyle.teal) }
                            if current.status == "completed" || current.status == "uncertain" {
                                HStack { Text("Provider-reported cost").foregroundStyle(MKStyle.muted); Spacer(); Text(current.actualCostMicrousd.map { String(format: "$%.6f", Double($0) / 1_000_000) } ?? "Unavailable").fontWeight(.semibold) }
                            }
                        }
                    }
                    if current.isDemo { Label("DEMO DATA  This payload was generated locally and was not sent to a provider.", systemImage: "sparkles").font(.footnote).foregroundStyle(.orange) }
                    if current.status == "uncertain" { Label("The provider may have processed this request and may have charged for it. MasterKey will not retry it automatically.", systemImage: "exclamationmark.triangle.fill").font(.footnote.weight(.medium)).foregroundStyle(.orange).padding(12).background(Color.orange.opacity(0.12), in: RoundedRectangle(cornerRadius: 12)) }
                    if current.status == "sending" { Label("This request is being sent to the provider.", systemImage: "arrow.up.forward.circle").font(.footnote).foregroundStyle(MKStyle.teal) }
                    if loadingDetail { ProgressView("Loading exact request details…").tint(MKStyle.teal) }
                    Text("FULL REQUEST JSON").font(.caption.bold()).tracking(1.1).foregroundStyle(MKStyle.muted)
                    if let payload = current.payload {
                        ScrollView([.horizontal, .vertical]) { Text(payload.prettyPrinted).font(.system(size: 12, weight: .regular, design: .monospaced)).foregroundStyle(MKStyle.cream).frame(maxWidth: .infinity, alignment: .leading).textSelection(.enabled).padding(14) }.frame(minHeight: 170, maxHeight: 360).background(MKStyle.codeBackground, in: RoundedRectangle(cornerRadius: 14))
                    } else { Text("Full payload unavailable. Refresh before making a decision.").font(.footnote).foregroundStyle(.orange).padding(12).frame(maxWidth: .infinity, alignment: .leading).background(Color.orange.opacity(0.1), in: RoundedRectangle(cornerRadius: 12)) }
                    Text("Cost is a conservative reservation estimate, not a guarantee of the provider’s final charge.").font(.caption).foregroundStyle(MKStyle.muted)
                    if current.status != "pending" {
                        Text("FULL RESPONSE JSON").font(.caption.bold()).tracking(1.1).foregroundStyle(MKStyle.muted)
                    }
                    if let response = current.response {
                        ScrollView(.horizontal) { Text(response.prettyPrinted).font(.system(size: 12, design: .monospaced)).foregroundStyle(MKStyle.cream).frame(maxWidth: .infinity, alignment: .leading).textSelection(.enabled).padding(14) }.background(MKStyle.codeBackground, in: RoundedRectangle(cornerRadius: 14))
                    } else if ["approved", "sending"].contains(current.status) {
                        Text("Waiting for the provider response.").font(.footnote).foregroundStyle(MKStyle.muted)
                    } else if current.status != "pending", !loadingDetail {
                        Text(current.error ?? "No provider response was recorded.").font(.footnote).foregroundStyle(MKStyle.muted)
                    }
                    if current.status == "pending" {
                        HStack(spacing: 12) {
                            Button { Task { await store.decide(current, approve: false); dismiss() } } label: { Text("Deny").fontWeight(.semibold).frame(maxWidth: .infinity).padding(15).background(.white.opacity(0.08), in: Capsule()) }.tint(MKStyle.cream)
                            Button { showPolicy = true } label: { Text("Approve").fontWeight(.bold).frame(maxWidth: .infinity).padding(15).background(MKStyle.teal, in: Capsule()).foregroundStyle(MKStyle.navy) }.disabled(current.payload == nil || loadingDetail)
                        }
                    }
                }.padding(20)
            }.background(MKStyle.navy.ignoresSafeArea()).navigationTitle("Request detail").navigationBarTitleDisplayMode(.inline).toolbar { ToolbarItem(placement: .topBarTrailing) { Button("Done") { dismiss() } } }
                .sheet(isPresented: $showPolicy) {
                    ApprovalPolicyView { policy in
                        showPolicy = false
                        Task { await store.decide(current, approve: true, policy: policy) }
                    }
                }
                .task(id: "\(request.id):\(current.status)") { loadingDetail = true; await store.loadRequestDetails(id: current.id); loadingDetail = false }
                .refreshable { await store.loadRequestDetails(id: current.id) }
        }
    }
}

struct ApprovalPolicyView: View {
    @Environment(\.dismiss) private var dismiss
    @State private var mode = "ask"
    @State private var count = 5
    @State private var budget = 1.0
    @State private var alert = 0.5
    var onApprove: (Policy) -> Void
    var body: some View {
        NavigationStack {
            Form {
                Section("Approval scope") {
                    Picker("Allow", selection: $mode) { Text("Once").tag("ask"); Text("Next calls").tag("count"); Text("Budget").tag("budget"); Text("Trust this app").tag("trust") }
                    if mode == "count" { Stepper("\(count) requests", value: $count, in: 1...100) }
                    if mode == "budget" { HStack { Text("Total budget"); Spacer(); TextField("USD", value: $budget, format: .number.precision(.fractionLength(2))).keyboardType(.decimalPad).multilineTextAlignment(.trailing).frame(width: 100) } }
                    if mode == "trust" { HStack { Text("Daily alert"); Spacer(); TextField("USD", value: $alert, format: .number.precision(.fractionLength(2))).keyboardType(.decimalPad).multilineTextAlignment(.trailing).frame(width: 100) } }
                }
                Section { Text("You can change or revoke app grants any time. Cost estimates are display estimates.").font(.footnote).foregroundStyle(MKStyle.muted) }
                Section {
                    Button("Approve") { onApprove(Policy(mode: mode, remainingCount: mode == "count" ? count : nil, remainingUSD: mode == "budget" ? budget : nil, alertUSDPerDay: mode == "trust" ? alert : nil)) }
                        .fontWeight(.semibold)
                        .frame(maxWidth: .infinity)
                        .buttonStyle(.borderedProminent)
                        .tint(MKStyle.teal)
                        .foregroundStyle(MKStyle.navy)
                }
            }.scrollContentBackground(.hidden).background(MKStyle.navy).navigationTitle("Approve request").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
        }.presentationDetents([.medium, .large])
    }
}

struct VaultView: View {
    @EnvironmentObject private var store: AppStore
    @State private var provider: Provider = .openAI
    @State private var secret = ""
    var body: some View {
        ScrollView { VStack(alignment: .leading, spacing: 18) {
            PageHeader(eyebrow: "Private by design", title: "Key vault", subtitle: "MasterKey stores the key. Apps only receive permission to use it.")
            ModeBanner(); InfoBanner()
            Card { VStack(alignment: .leading, spacing: 12) {
                Text("ADD A PROVIDER KEY").font(.caption.bold()).tracking(1).foregroundStyle(MKStyle.teal)
                Picker("Provider", selection: $provider) { ForEach(Provider.allCases) { Text($0.title).tag($0) } }.pickerStyle(.segmented)
                SecureField("Paste API key", text: $secret).textFieldStyle(.roundedBorder).textInputAutocapitalization(.never).autocorrectionDisabled()
                Button { let value = secret; secret = ""; Task { await store.addKey(provider: provider, value: value, label: provider.title) } } label: { Label(store.demoMode ? "Live server required" : "Save key securely", systemImage: "lock.fill").frame(maxWidth: .infinity).padding(13).background(MKStyle.teal, in: Capsule()).foregroundStyle(MKStyle.navy).fontWeight(.bold) }.disabled(store.demoMode || secret.isEmpty)
                Text("The key is sent only to your configured HTTPS server and is never shown again. Demo mode does not store or transmit key values.").font(.caption).foregroundStyle(MKStyle.muted)
            } }
            Text("SAVED KEYS").font(.caption.bold()).tracking(1).foregroundStyle(MKStyle.muted)
            ForEach(store.keys) { key in Card { HStack(spacing: 12) { Image(systemName: "key.fill").foregroundStyle(MKStyle.teal); VStack(alignment: .leading, spacing: 4) { Text(key.label).font(.headline); Text("\(key.provider.title)  •  ending \(key.lastFour)").font(.caption).foregroundStyle(MKStyle.muted) }; Spacer(); Button(role: .destructive) { Task { await store.removeKey(key) } } label: { Image(systemName: "trash") }.accessibilityLabel("Remove \(key.label) key") } } }
            if store.keys.isEmpty { Card { Text("No keys added yet.").foregroundStyle(MKStyle.muted) } }
        }.padding(20) }.background(MKStyle.navy.ignoresSafeArea()).navigationBarHidden(true)
    }
}

struct GrantsView: View {
    @EnvironmentObject private var store: AppStore
    @State private var selected: GrantRecord?
    var body: some View {
        ScrollView { VStack(alignment: .leading, spacing: 18) {
            PageHeader(eyebrow: "Access under your control", title: "App grants", subtitle: "Review usage, limits, expiry, and revoke access.")
            ModeBanner(); InfoBanner()
            if store.grants.isEmpty { Card { Text("No active app grants.").foregroundStyle(MKStyle.muted) } }
            ForEach(store.grants) { grant in
                Button { selected = grant } label: { GrantCard(grant: grant) }.buttonStyle(.plain)
            }
            Text("Tap a grant to change its policy or revoke access.").font(.caption).foregroundStyle(MKStyle.muted)
        }.padding(20) }.background(MKStyle.navy.ignoresSafeArea()).navigationBarHidden(true)
            .sheet(item: $selected) { item in GrantDetailView(grant: store.grants.first(where: { $0.id == item.id }) ?? item) }
    }
}

struct GrantCard: View {
    let grant: GrantRecord
    var body: some View {
        Card { VStack(alignment: .leading, spacing: 14) {
            HStack { VStack(alignment: .leading, spacing: 4) { Text(grant.appName).font(.headline).foregroundStyle(MKStyle.cream); Text(grant.model.isEmpty ? grant.provider.title : "\(grant.provider.title)  /  \(grant.model)").font(.caption).foregroundStyle(MKStyle.muted) }; Spacer(); Image(systemName: "chevron.right").font(.caption.bold()).foregroundStyle(MKStyle.muted) }
            HStack(spacing: 14) {
                metric("REQUESTS", value: grant.maxRequests.map { "\($0) left" } ?? "\(grant.usedRequests) used")
                Rectangle().fill(.white.opacity(0.1)).frame(width: 1, height: 32)
                metric("SPENT", value: grant.spentUSD)
                Rectangle().fill(.white.opacity(0.1)).frame(width: 1, height: 32)
                metric("POLICY", value: grant.trust ? "Trusted" : (grant.budgetMicrousd != nil ? "Budget" : "Count"))
            }
            if let remaining = grant.budgetMicrousd { Text(String(format: "$%.2f remaining", Double(remaining) / 1_000_000)).font(.caption).foregroundStyle(MKStyle.teal) }
            Text(grant.expiresAt.isEmpty ? "No expiry" : "Expires \(grant.expiresAt.asDate)").font(.caption).foregroundStyle(MKStyle.muted)
        } }
    }
    func metric(_ name: String, value: String) -> some View { VStack(alignment: .leading, spacing: 4) { Text(name).font(.system(size: 9, weight: .bold)).tracking(0.8).foregroundStyle(MKStyle.muted); Text(value).font(.subheadline.weight(.semibold)).foregroundStyle(MKStyle.cream) }.frame(maxWidth: .infinity, alignment: .leading) }
}

struct GrantDetailView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    @State private var showPolicy = false
    let grant: GrantRecord
    var body: some View {
        NavigationStack { VStack(alignment: .leading, spacing: 18) {
            GrantCard(grant: grant)
            Button { showPolicy = true } label: { Label("Change policy", systemImage: "slider.horizontal.3").frame(maxWidth: .infinity).padding(14).background(.white.opacity(0.08), in: Capsule()) }
            Button(role: .destructive) { Task { await store.revoke(grant); dismiss() } } label: { Label("Revoke grant", systemImage: "hand.raised.slash").frame(maxWidth: .infinity).padding(14).background(Color.red.opacity(0.16), in: Capsule()) }
            Spacer()
        }.padding(20).background(MKStyle.navy.ignoresSafeArea()).navigationTitle(grant.appName).navigationBarTitleDisplayMode(.inline).toolbar { ToolbarItem(placement: .topBarTrailing) { Button("Done") { dismiss() } } }.sheet(isPresented: $showPolicy) { ApprovalPolicyView { policy in Task { await store.update(grant, policy: policy); showPolicy = false } } } }
    }
}

struct AccountView: View {
    @EnvironmentObject private var store: AppStore
    @State private var stagingUsername = ""
    @State private var stagingPassword = ""

    var body: some View {
        Form {
            Section {
                PageHeader(eyebrow: "MasterKey", title: "Account", subtitle: "Manage your sign-in and app appearance.")
            }.listRowBackground(MKStyle.navy)
            Section("Profile") {
                if store.demoMode {
                    Label("Demo workspace", systemImage: "person.crop.circle")
                    Text("Turn off Demo mode in Settings to sign in.").font(.footnote).foregroundStyle(MKStyle.muted)
                } else if let account = store.account {
                    if let email = account.displayEmail {
                        LabeledContent("Email", value: email)
                    }
                    LabeledContent("Account ID", value: account.id)
                    LabeledContent("Server", value: store.serverURL)
                    Button("Sign out", role: .destructive) { store.signOut() }
                } else {
                    AppleAuthenticationButton(mode: .signIn)
                    NavigationLink("Sign up") { SignUpView() }
                }
            }
            if !store.demoMode, store.account == nil, store.supportsStagingPassword {
                Section("Staging sign-in") {
                    TextField("Username", text: $stagingUsername)
                        .textContentType(.username).textInputAutocapitalization(.never).autocorrectionDisabled()
                    SecureField("Password", text: $stagingPassword)
                        .textContentType(.password)
                    Button("Sign in with password") {
                        let username = stagingUsername.trimmingCharacters(in: .whitespacesAndNewlines)
                        let password = stagingPassword
                        stagingPassword = ""
                        Task { await store.signInPassword(username: username, password: password) }
                    }.disabled(store.busy || stagingUsername.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || stagingPassword.isEmpty)
                }
            }
            if !store.demoMode, store.account == nil, !store.supportsStagingPassword {
                Section("Development sign-in") {
                    TextField("Developer email", text: $store.email)
                        .keyboardType(.emailAddress).textInputAutocapitalization(.never).autocorrectionDisabled()
                    Button("Sign in to development server") {
                        Task { store.saveSettings(); await store.signIn() }
                    }.disabled(store.busy)
                    Text("Available when the server enables development login.").font(.footnote).foregroundStyle(MKStyle.muted)
                }
            }
            Section("Appearance") { AppearancePicker() }
        }.scrollContentBackground(.hidden).background(MKStyle.navy.ignoresSafeArea()).navigationBarHidden(true)
    }
}

private struct SignUpView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        Form {
            Section {
                AppleAuthenticationButton(mode: .signUp)
            }
            if store.supportsStagingPassword {
                Section {
                    Text("For password sign-in on staging, ask the server operator for a test account.")
                        .font(.footnote).foregroundStyle(MKStyle.muted)
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(MKStyle.navy.ignoresSafeArea())
        .navigationTitle("Sign up")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.visible, for: .navigationBar)
        .onChange(of: store.account?.id) { _, accountID in
            if accountID != nil { dismiss() }
        }
    }
}

private struct AppleAuthenticationButton: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var colorScheme
    let mode: SignInWithAppleButton.Label

    var body: some View {
        SignInWithAppleButton(mode, onRequest: { $0.requestedScopes = [.email] }, onCompletion: handleAppleSignIn)
            .signInWithAppleButtonStyle(colorScheme == .dark ? .white : .black)
            .frame(height: 46)
            .disabled(store.busy)
    }

    private func handleAppleSignIn(_ result: Result<ASAuthorization, Error>) {
        do {
            let authorization = try result.get()
            guard let credential = authorization.credential as? ASAuthorizationAppleIDCredential,
                  let tokenData = credential.identityToken,
                  let idToken = String(data: tokenData, encoding: .utf8) else {
                store.errorMessage = "Apple did not return an identity token."
                store.showError = true
                return
            }
            Task { await store.signInApple(idToken: idToken) }
        } catch {
            if (error as? ASAuthorizationError)?.code == .canceled { return }
            store.errorMessage = error.localizedDescription
            store.showError = true
        }
    }
}

struct SettingsView: View {
    @EnvironmentObject private var store: AppStore
    @State private var code = ""
    @State private var serverDraft = UserDefaults.standard.string(forKey: "serverURL") ?? "https://localhost:8443"
    @State private var pairing: (code: String, preview: ConnectPreview)?
    @State private var showPairPolicy = false
    @State private var decidingPairing = false
    private var enteredCode: String { code.trimmingCharacters(in: .whitespacesAndNewlines).uppercased() }
    var body: some View {
        Form {
            Section { PageHeader(eyebrow: "Connection", title: "Settings", subtitle: "Choose demo mode or connect to your server.") }.listRowBackground(MKStyle.navy)
            Section("Workspace") {
                Toggle("Demo mode", isOn: Binding(get: { store.demoMode }, set: { value in Task { await store.setDemo(value) } }))
                if store.demoMode { Text("Sample requests and grants are stored only in memory. Actions are local simulations.").font(.footnote).foregroundStyle(.orange) }
                TextField("Server URL", text: $serverDraft).keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled().disabled(store.demoMode)
                Text("Live server URLs must use HTTPS. Enable the localhost development option below for a local HTTP server. The demo workspace stays offline.").font(.footnote).foregroundStyle(MKStyle.muted)
                if !store.demoMode {
                    Button("Use this server") { store.serverURL = serverDraft.trimmingCharacters(in: .whitespacesAndNewlines); store.serverChanged(); store.saveSettings() }.disabled(serverDraft == store.serverURL)
                    Toggle("Allow HTTP for localhost", isOn: $store.allowLocalHTTP).onChange(of: store.allowLocalHTTP) { _, _ in store.saveSettings() }
                    Text("Use only for a local development server. Remote HTTP URLs are always rejected.").font(.footnote).foregroundStyle(MKStyle.muted)
                    if store.account == nil {
                        Text("Sign in from the Account tab.").font(.footnote).foregroundStyle(MKStyle.muted)
                    }
                }
            }
            Section("Connect an app") {
                HStack {
                    TextField("Code, e.g. MK-4F7Q", text: $code).textInputAutocapitalization(.characters).autocorrectionDisabled()
                    Button("Preview") { loadCode(enteredCode) }.disabled(enteredCode.isEmpty || decidingPairing)
                }
                if let pairing, pairing.code == enteredCode {
                    let preview = pairing.preview
                    VStack(alignment: .leading, spacing: 8) {
                        Text(preview.app.name).font(.title3.bold())
                        if let host = preview.sourceHost {
                            Label("From \(host)", systemImage: "globe").font(.headline).textSelection(.enabled)
                            Text("Check that this host matches the page you’re using before approving.").font(.footnote).foregroundStyle(MKStyle.muted)
                        } else {
                            Label("Unverified app: name chosen by the developer", systemImage: "exclamationmark.triangle")
                                .font(.footnote.weight(.semibold)).foregroundStyle(MKStyle.warning)
                        }
                        Text("Requests access to your \(preview.vendor.title) key.").font(.subheadline).foregroundStyle(MKStyle.muted)
                        if preview.requiresAsk != false {
                            Text("Approve calls one at a time. You can widen access later in Grants.").font(.footnote).foregroundStyle(MKStyle.muted)
                        }
                        HStack {
                            Button("Deny", role: .destructive) { decidePairing(code: pairing.code, approve: false) }
                            Spacer()
                            if preview.requiresAsk == false {
                                Button("Choose access") { showPairPolicy = true }.fontWeight(.bold)
                            } else {
                                Button("Connect and ask each time") { decidePairing(code: pairing.code, approve: true) }.fontWeight(.bold)
                            }
                        }.buttonStyle(.borderless).disabled(decidingPairing)
                    }
                }
            }
            Section("Notifications") {
                Button { Task { await store.requestNotifications() } } label: { Label("Enable push notifications", systemImage: "bell.badge") }
                Text("Push needs valid Apple signing and server APNs credentials. Requests also refresh when you open the app.").font(.footnote).foregroundStyle(MKStyle.muted)
            }
            Section { InfoBanner() }.listRowBackground(MKStyle.navy)
        }.scrollContentBackground(.hidden).background(MKStyle.navy.ignoresSafeArea()).navigationBarHidden(true).tint(MKStyle.teal)
            .onAppear { if let value = store.pendingConnectCode { loadCode(value) } }
            .onChange(of: store.pendingConnectCode) { _, value in
                guard let value else { return }; loadCode(value)
            }
            .onChange(of: store.contextGeneration) { _, _ in pairing = nil; showPairPolicy = false }
            .sheet(isPresented: $showPairPolicy) {
                ApprovalPolicyView { policy in
                    showPairPolicy = false
                    guard let pairing, pairing.code == enteredCode else { return }
                    decidePairing(code: pairing.code, approve: true, policy: policy)
                }
            }
    }
    private func loadCode(_ value: String) {
        code = value
        pairing = nil
        store.pendingConnectCode = nil
        Task {
            guard let preview = await store.connectPreview(code: value), enteredCode == value else { return }
            pairing = (value, preview)
        }
    }
    private func decidePairing(code: String, approve: Bool, policy: Policy = .once) {
        decidingPairing = true
        Task {
            defer { decidingPairing = false }
            if await store.connectDecision(code: code, approve: approve, policy: policy), pairing?.code == code {
                pairing = nil
            }
        }
    }
}

private extension String {
    var asDate: String {
        let formatter = ISO8601DateFormatter()
        guard let date = formatter.date(from: self) else { return self }
        let output = DateFormatter(); output.dateStyle = .medium; output.timeStyle = .short
        return output.string(from: date)
    }
}
