import SwiftUI
import AuthenticationServices
import VisionKit
import AVFoundation

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
                if !showHistory && store.keys.isEmpty {
                    Card { VStack(alignment: .leading, spacing: 12) {
                        Text("Add your first provider").font(.headline)
                        Text("Set up a provider key once, then connect the apps you want to use.").font(.subheadline).foregroundStyle(MKStyle.muted)
                        Button("Add a provider") { NotificationCenter.default.post(name: .masterKeyOpenKeys, object: nil) }.buttonStyle(.borderedProminent)
                    } }
                } else if !showHistory && store.grants.isEmpty {
                    Card { VStack(alignment: .leading, spacing: 12) {
                        Text("Connect your first app").font(.headline)
                        Text("Scan the connection QR shown by an app’s website.").font(.subheadline).foregroundStyle(MKStyle.muted)
                        Button("Scan connection QR") { NotificationCenter.default.post(name: .masterKeyConnectApp, object: nil) }.buttonStyle(.borderedProminent)
                    } }
                } else if visible.isEmpty {
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
        }.background(MKStyle.navy.ignoresSafeArea()).navigationBarTitleDisplayMode(.inline)
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
        if store.demoMode {
            Label("Sample data: offline, no provider calls or charges", systemImage: "testtube.2")
                .font(.footnote).foregroundStyle(MKStyle.warning).padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(MKStyle.warning.opacity(0.1), in: RoundedRectangle(cornerRadius: 12))
        } else if store.serverURL != AppPreferences.productionURL {
            Label("Development server", systemImage: "wrench.and.screwdriver")
                .font(.caption).foregroundStyle(MKStyle.warning)
        }
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
    var returnConnectCode: String? = nil
    @EnvironmentObject private var store: AppStore
    @State private var provider: Provider = .openAI
    @State private var secret = ""
    @State private var saving = false
    var body: some View {
        ScrollView { VStack(alignment: .leading, spacing: 18) {
            PageHeader(eyebrow: "Providers", title: "Your keys", subtitle: "Connect a provider once. Choose which apps can use it.")
            ModeBanner(); InfoBanner()
            Card { VStack(alignment: .leading, spacing: 12) {
                Text("Add from your computer").font(.headline)
                Text("Create and paste the key on your computer. Scan its QR here, then confirm the key on this phone.").font(.subheadline).foregroundStyle(MKStyle.muted)
                Button { NotificationCenter.default.post(name: .masterKeyAddFromComputer, object: returnConnectCode) } label: { Label("Add from computer", systemImage: "desktopcomputer").frame(maxWidth: .infinity) }.buttonStyle(.borderedProminent).disabled(store.demoMode)
            } }
            ForEach(store.keys) { key in Card { HStack(spacing: 12) {
                Image(systemName: "key.fill").foregroundStyle(MKStyle.teal)
                VStack(alignment: .leading, spacing: 4) { Text(key.provider.title).font(.headline); Text("Key ending \(key.lastFour)").font(.caption).foregroundStyle(MKStyle.muted) }
                Spacer()
                Button(role: .destructive) { Task { await store.removeKey(key) } } label: { Image(systemName: "trash") }.accessibilityLabel("Remove \(key.provider.title) key")
            } } }
            Card { VStack(alignment: .leading, spacing: 14) {
                Text("Or paste a key on this phone").font(.headline)
                Picker("Provider", selection: $provider) { ForEach(Provider.allCases) { Text($0.title).tag($0) } }.pickerStyle(.segmented).disabled(saving)
                Link(destination: provider.keysURL) { Label("Create a key at \(provider.title)", systemImage: "arrow.up.right.square") }
                Text("Copy a key from the provider, then paste it below. On a Mac and iPhone using the same Apple Account, Universal Clipboard can save you typing.").font(.footnote).foregroundStyle(MKStyle.muted)
                HStack {
                    SecureField(provider.keyPlaceholder, text: $secret).textFieldStyle(.roundedBorder).textInputAutocapitalization(.never).autocorrectionDisabled().privacySensitive().disabled(saving)
                    PasteButton(payloadType: String.self) { values in
                        if let value = values.first { secret = value.trimmingCharacters(in: .whitespacesAndNewlines) }
                    }.labelStyle(.iconOnly).disabled(saving || store.demoMode)
                }
                Button {
                    let value = secret; let selectedProvider = provider; let generation = store.contextGeneration
                    saving = true
                    Task {
                        defer { saving = false }
                        if await store.addKey(provider: selectedProvider, value: value, label: selectedProvider.title),
                           generation == store.contextGeneration, secret == value { secret = "" }
                    }
                } label: {
                    HStack { if saving { ProgressView() }; Text(saving ? "Saving…" : "Save key").fontWeight(.semibold) }.frame(maxWidth: .infinity).padding(13)
                }.buttonStyle(.borderedProminent).tint(MKStyle.teal).foregroundStyle(MKStyle.navy)
                    .disabled(store.demoMode || saving || secret.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Text("Sent over HTTPS to your MasterKey server and encrypted there. Connected apps never receive the key. Saving does not make a billable model call.").font(.caption).foregroundStyle(MKStyle.muted)
            } }
        }.padding(20) }.background(MKStyle.navy.ignoresSafeArea()).navigationBarTitleDisplayMode(.inline)
            .onChange(of: provider) { _, _ in secret = "" }
            .onChange(of: store.contextGeneration) { _, _ in secret = "" }
            .onDisappear { secret = "" }
    }
}

struct GrantsView: View {
    @EnvironmentObject private var store: AppStore
    @State private var selected: GrantRecord?
    var body: some View {
        ScrollView { VStack(alignment: .leading, spacing: 18) {
            PageHeader(eyebrow: "Access under your control", title: "Connected apps", subtitle: "Choose which apps can use your providers.")
            ModeBanner(); InfoBanner()
            if store.grants.isEmpty { Card { Text("Connect an app using the QR code on its website. You’ll review the source before allowing access.").foregroundStyle(MKStyle.muted) } }
            ForEach(store.grants) { grant in
                Button { selected = grant } label: { GrantCard(grant: grant) }.buttonStyle(.plain)
            }
            Text("Tap a grant to change its policy or revoke access.").font(.caption).foregroundStyle(MKStyle.muted)
        }.padding(20) }.background(MKStyle.navy.ignoresSafeArea()).navigationBarTitleDisplayMode(.inline)
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
                metric("POLICY", value: grant.trust ? "Trusted" : (grant.budgetMicrousd != nil ? "Budget" : (grant.maxRequests != nil ? "Count" : "Ask each time")))
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
                PageHeader(eyebrow: "MasterKey", title: "Account", subtitle: "Manage your sign-in.")
            }.listRowBackground(MKStyle.navy)
            Section("Profile") {
                if store.demoMode {
                    Label("Sample data", systemImage: "person.crop.circle")
                    Text("Turn off sample data in Developer options to sign in.").font(.footnote).foregroundStyle(MKStyle.muted)
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
            if store.developerOptions, !store.demoMode, store.account == nil, !store.supportsStagingPassword, store.serverURL != AppPreferences.productionURL {
                Section("Development sign-in") {
                    TextField("Developer email", text: $store.email)
                        .keyboardType(.emailAddress).textInputAutocapitalization(.never).autocorrectionDisabled()
                    Button("Sign in to development server") {
                        Task { store.saveSettings(); await store.signIn() }
                    }.disabled(store.busy)
                    Text("Available when the server enables development login.").font(.footnote).foregroundStyle(MKStyle.muted)
                }
            }
        }.scrollContentBackground(.hidden).background(MKStyle.navy.ignoresSafeArea()).navigationBarTitleDisplayMode(.inline)
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

struct AppleAuthenticationButton: View {
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

struct WelcomeView: View {
    @EnvironmentObject private var store: AppStore
    var body: some View {
        ScrollView { VStack(alignment: .leading, spacing: 28) {
            Image(systemName: "key.horizontal").font(.system(size: 48)).foregroundStyle(MKStyle.teal).padding(.top, 36)
            PageHeader(eyebrow: "MasterKey", title: "Your keys.\nYour permission.", subtitle: "Use AI apps with your own provider account. Review what each app sends before it runs.")
            VStack(alignment: .leading, spacing: 18) {
                Label("Add a provider key once", systemImage: "1.circle")
                Label("Scan an app’s connection QR", systemImage: "2.circle")
                Label("Review and approve requests", systemImage: "3.circle")
            }.font(.subheadline)
            AppleAuthenticationButton(mode: .continue)
            Text("New here? Continuing creates your MasterKey account.").font(.footnote).foregroundStyle(MKStyle.muted)
            if store.developerOptions { NavigationLink("Other sign-in options") { AccountView() } }
            InfoBanner()
        }.padding(24) }.background(MKStyle.navy.ignoresSafeArea())
    }
}

struct SettingsView: View {
    @EnvironmentObject private var store: AppStore
    var body: some View {
        Form {
            Section("Account") { NavigationLink { AccountView() } label: { Label(store.account == nil ? "Sign in" : "Account", systemImage: "person.crop.circle") } }
            Section("Appearance") { AppearancePicker() }
            Section("Notifications") {
                Button { Task { await store.requestNotifications() } } label: { Label("Enable notifications", systemImage: "bell.badge") }
                Text("Get notified when a request needs your approval. Requests also refresh when you open the app.").font(.footnote).foregroundStyle(MKStyle.muted)
            }
            Section { NavigationLink("Developer options") { DeveloperOptionsView() } }
            Section { InfoBanner() }
        }.scrollContentBackground(.hidden).background(MKStyle.navy).navigationTitle("Settings")
    }
}

struct DeveloperOptionsView: View {
    @EnvironmentObject private var store: AppStore
    @State private var serverDraft = ""
    var body: some View {
        Form {
            Section {
                Toggle("Enable developer options", isOn: $store.developerOptions).disabled(store.demoMode || store.serverURL != AppPreferences.productionURL).onChange(of: store.developerOptions) { _, _ in store.saveSettings() }
                Text(store.demoMode || store.serverURL != AppPreferences.productionURL ? "Turn off sample data and select production before hiding developer options." : "For building and testing integrations. Normal use connects to MasterKey automatically.").font(.footnote).foregroundStyle(MKStyle.muted)
            }
            if store.developerOptions {
                Section("Offline samples") {
                    Toggle("Use sample data", isOn: Binding(get: { store.demoMode }, set: { value in Task { await store.setDemo(value) } }))
                    Text("Fake requests and grants on this device only. No network requests, real keys, provider calls or charges. Switching this setting signs you out.").font(.footnote).foregroundStyle(MKStyle.muted)
                    Text("To test a real integration without provider charges, run a private backend with MK_VENDOR_MOCK=1 and fake keys. This switch cannot change a server’s mock setting.").font(.footnote).foregroundStyle(MKStyle.muted)
                }
                Section("Server") {
                    Button("Use production") { store.useServer(AppPreferences.productionURL); serverDraft = store.serverURL }
                    Button("Use staging") { store.useServer(AppPreferences.stagingURL); serverDraft = store.serverURL }
                    TextField("Custom HTTPS URL", text: $serverDraft).keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
                    Button("Use custom server") { store.useServer(serverDraft) }.disabled(serverDraft == store.serverURL || store.demoMode)
                    Toggle("Allow HTTP for localhost", isOn: $store.allowLocalHTTP).onChange(of: store.allowLocalHTTP) { _, _ in store.saveSettings() }
                    Text("Changing servers signs you out. Remote servers require HTTPS; local HTTP is for a private development backend only.").font(.footnote).foregroundStyle(MKStyle.muted)
                    LabeledContent("Current", value: store.serverURL).font(.caption)
                }.disabled(store.demoMode)
                if !store.demoMode { Section { NavigationLink("Development sign-in") { AccountView() } } }
            }
        }.scrollContentBackground(.hidden).background(MKStyle.navy).navigationTitle("Developer options")
            .onAppear { serverDraft = store.serverURL }
    }
}

struct ConnectAppView: View {
    var addingKey = false
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    @State private var code = ""
    @State private var pairing: (code: String, generation: Int, preview: ConnectPreview)?
    @State private var showScanner = false
    @State private var scannerGeneration: Int?
    @State private var scannerLinkGeneration: Int?
    @State private var scannedValue: String?
    @State private var setupLink = ""
    @State private var scannerError: String?
    @State private var showPairPolicy = false
    @State private var deciding = false
    @State private var loading = false
    private var enteredCode: String? { PairingLink.code(from: code) }
    private var previewTaskID: String { "\(store.contextGeneration):\(store.account?.id ?? ""):\(code)" }

    var body: some View {
        Group {
            if store.account == nil && !store.demoMode {
                VStack(alignment: .leading, spacing: 16) {
                    Text("Sign in to review this connection").font(.headline)
                    Text("Your pairing link will stay here while you sign in.").foregroundStyle(MKStyle.muted)
                    AccountView()
                }.padding(.top)
            } else {
                Form {
                    Section {
                        Button { Task { await openScanner() } } label: { Label("Scan QR code", systemImage: "qrcode.viewfinder").font(.headline).frame(maxWidth: .infinity).padding(8) }.disabled(deciding)
                        Text(addingKey ? "Scan the key setup QR on your computer. You’ll approve this computer before it can send a key." : "On the app’s website, choose Connect with MasterKey. Scan its QR code here, or tap its link on this phone.").font(.footnote).foregroundStyle(MKStyle.muted)
                        if let scannerError { Text(scannerError).font(.footnote).foregroundStyle(MKStyle.warning) }
                    }
                    if addingKey {
                        Section("Can’t scan? Paste the setup link") {
                            HStack {
                                TextField("https://…/enroll/en_…", text: $setupLink).keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
                                PasteButton(payloadType: String.self) { values in if let value = values.first { setupLink = value.trimmingCharacters(in: .whitespacesAndNewlines) } }.labelStyle(.iconOnly)
                            }
                            Button("Review setup link") { _ = store.acceptEnrollment(setupLink) }.disabled(EnrollmentLink.id(from: setupLink) == nil)
                        }
                    } else { Section("Can’t scan? Enter the code") {
                        TextField("MK-4F7Q", text: $code).textInputAutocapitalization(.characters).autocorrectionDisabled().disabled(deciding)
                        if loading { ProgressView("Loading connection…") }
                        else if !code.isEmpty && enteredCode == nil { Text("Enter the MK- code shown by the app.").font(.footnote).foregroundStyle(MKStyle.muted) }
                    } }
                    if let pairing, pairing.code == enteredCode, pairing.generation == store.contextGeneration {
                        let preview = pairing.preview
                        Section("Review connection") {
                            Text(preview.app.name).font(.title2.bold())
                            if let host = preview.sourceHost {
                                Label("From \(host)", systemImage: "globe").font(.headline).textSelection(.enabled)
                                Text("Check that this host matches the page you’re using.").font(.footnote).foregroundStyle(MKStyle.muted)
                            } else {
                                Label("Unverified app: name chosen by the developer", systemImage: "exclamationmark.triangle").foregroundStyle(MKStyle.warning)
                            }
                            Text("Requests access to your \(preview.vendor.title) key.")
                            if !store.keys.contains(where: { $0.provider == preview.vendor }) {
                                NavigationLink("Add your \(preview.vendor.title) key first") { VaultView(returnConnectCode: pairing.code) }
                            }
                            Text("Each call needs your approval at first. Change access later in Connected apps.").font(.footnote).foregroundStyle(MKStyle.muted)
                            Button(preview.requiresAsk == false ? "Choose access" : "Connect and ask each time") {
                                if preview.requiresAsk == false { showPairPolicy = true } else { decide(approve: true) }
                            }.disabled(deciding || !store.keys.contains(where: { $0.provider == preview.vendor }))
                            Button("Deny connection", role: .destructive) { decide(approve: false) }.disabled(deciding)
                        }
                    }
                }.scrollContentBackground(.hidden)
            }
        }.background(MKStyle.navy).navigationTitle(addingKey ? "Scan key setup QR" : "Connect an app").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
            .onAppear { if let pending = store.pendingConnectCode { code = pending } }
            .onChange(of: store.pendingConnectCode) { _, value in if let value { code = value; showScanner = false } }
            .onChange(of: store.linkGeneration) { _, _ in scannedValue = nil; showScanner = false }
            .onChange(of: store.contextGeneration) { _, _ in pairing = nil; showPairPolicy = false; showScanner = false; scannerGeneration = nil }
            .task(id: previewTaskID) { await loadPreview() }
            .sheet(isPresented: $showPairPolicy) { ApprovalPolicyView { policy in showPairPolicy = false; decide(approve: true, policy: policy) } }
            .sheet(isPresented: $showScanner, onDismiss: processScannedValue) {
                NavigationStack {
                    QRScannerView { value in
                        guard showScanner, scannerGeneration == store.contextGeneration, scannerLinkGeneration == store.linkGeneration else { return }
                        scannedValue = value
                        showScanner = false
                    } onError: {
                        guard showScanner, scannerGeneration == store.contextGeneration else { return }
                        scannerError = addingKey ? "Camera scanning is unavailable. Paste the setup link below." : "Camera scanning is unavailable. Enter the connection code instead."; showScanner = false
                    }
                    .ignoresSafeArea(edges: .bottom).navigationTitle(addingKey ? "Scan key setup QR" : "Scan MasterKey QR")
                    .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { showScanner = false } } }
                }
            }
    }
    private func loadPreview() async {
        pairing = nil
        loading = false
        guard let value = enteredCode, store.account != nil || store.demoMode else { return }
        let generation = store.contextGeneration
        loading = true
        defer { if generation == store.contextGeneration { loading = false } }
        guard let preview = await store.connectPreview(code: value), !Task.isCancelled,
              generation == store.contextGeneration, enteredCode == value else { return }
        pairing = (value, generation, preview)
    }
    private func decide(approve: Bool, policy: Policy = .once) {
        guard let pairing, pairing.code == enteredCode, pairing.generation == store.contextGeneration else { return }
        deciding = true
        Task {
            defer { deciding = false }
            if await store.connectDecision(code: pairing.code, approve: approve, policy: policy),
               enteredCode == pairing.code, pairing.generation == store.contextGeneration {
                if store.pendingConnectCode == pairing.code { store.pendingConnectCode = nil }
                dismiss()
            }
        }
    }
    private func processScannedValue() {
        guard let value = scannedValue, scannerGeneration == store.contextGeneration, scannerLinkGeneration == store.linkGeneration else { return }
        scannedValue = nil
        code = ""; pairing = nil
        if EnrollmentLink.id(from: value) != nil { _ = store.acceptEnrollment(value) }
        else if PairingLink.code(from: value) != nil {
            if store.acceptPairing(value), let scanned = store.pendingConnectCode { code = scanned }
        } else { scannerError = "This isn’t a MasterKey QR. Scan the connection or key setup QR shown by MasterKey." }
    }

    private func openScanner() async {
        let generation = store.contextGeneration
        guard DataScannerViewController.isSupported else { scannerError = addingKey ? "QR scanning isn’t available on this device. Paste the setup link below instead." : "QR scanning isn’t available on this device. Enter the code instead."; return }
        guard await AVCaptureDevice.requestAccess(for: .video) else { scannerError = addingKey ? "Camera access is off. Enable it in iPhone Settings, or paste the setup link below." : "Camera access is off. Enable it in iPhone Settings, or enter the code instead."; return }
        guard DataScannerViewController.isAvailable else { scannerError = addingKey ? "Camera scanning is unavailable. Paste the setup link below." : "Camera scanning is unavailable. Enter the code instead."; return }
        guard generation == store.contextGeneration else { return }
        scannerError = nil
        scannerGeneration = generation
        scannerLinkGeneration = store.linkGeneration
        scannedValue = nil
        showScanner = true
    }
}

struct ComputerKeySetupView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        Form {
            Section("On your computer") {
                Text("Open this address in your computer’s browser:")
                if let url = store.computerSetupURL {
                    Text(url.absoluteString).font(.callout.monospaced()).textSelection(.enabled)
                    ShareLink(item: url) { Label("Send setup link", systemImage: "square.and.arrow.up") }
                }
                Text("Choose a provider. The page will show a QR code; no MasterKey password is needed on the computer.").font(.footnote).foregroundStyle(MKStyle.muted)
            }
            Section("On this phone") {
                NavigationLink { ConnectAppView(addingKey: true) } label: { Label("Scan setup QR", systemImage: "qrcode.viewfinder").font(.headline) }
                Text("Check the server and provider, then approve the computer. Paste the key on the computer and confirm its last four characters here before saving.").font(.footnote).foregroundStyle(MKStyle.muted)
            }
            Section {
                Text("Only a short-lived setup identifier is in the QR. Your key stays out of QR codes and phone enrollment screens.").font(.footnote).foregroundStyle(MKStyle.muted)
            }
        }.scrollContentBackground(.hidden).background(MKStyle.navy).navigationTitle("Add from computer").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
    }
}

struct EnrollmentView: View {
    let id: String
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    @State private var enrollment: KeyEnrollment?
    @State private var loading = false
    @State private var busy = false
    @State private var saved = false
    @State private var cancelled = false
    @State private var errorMessage: String?
    @State private var reload = 0
    @State private var viewToken = UUID()
    @State private var actionTask: Task<Void, Never>?
    private var loadID: String { "\(id):\(store.contextGeneration):\(store.account?.id ?? ""):\(reload)" }

    var body: some View {
        Group {
            if store.account == nil {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Sign in to add this key").font(.headline)
                    Text("The setup link will stay here while you sign in.").font(.footnote).foregroundStyle(MKStyle.muted)
                    AccountView()
                }.padding(.top)
            } else {
                Form {
                    Section("Key setup server") {
                        Label(store.serverHost, systemImage: "lock.shield").font(.headline).textSelection(.enabled)
                        Text("Check that this matches the address open on your computer. Approve only a setup you started yourself.").font(.footnote).foregroundStyle(MKStyle.muted)
                    }
                    if let errorMessage {
                        Section {
                            Text(errorMessage).foregroundStyle(MKStyle.warning)
                            Button("Check setup status") { reload += 1 }.disabled(busy)
                        }
                    }
                    if loading && enrollment == nil { Section { ProgressView("Loading key setup…") } }
                    if saved || enrollment?.status == "saved" {
                        Section {
                            Label("Key saved", systemImage: "checkmark.circle.fill").font(.headline).foregroundStyle(MKStyle.teal)
                            Text("Your apps can use this provider with your permission. They never receive the key.").font(.footnote).foregroundStyle(MKStyle.muted)
                            if store.canResumePairing {
                                Button("Continue connecting app") { continuePairing() }.fontWeight(.semibold).disabled(busy)
                            }
                            Button("Done") { dismiss() }
                        }
                    } else if cancelled || enrollment?.status == "cancelled" {
                        Section { Text("Key setup cancelled. Nothing from this setup will be saved.") }
                    } else if let enrollment {
                        Section(enrollment.vendor.title) {
                            if enrollment.isExpired || enrollment.status == "expired" {
                                Text("This setup has expired. Start a new setup on your computer.")
                            } else if enrollment.canClaim {
                                Text("Allow the computer showing this QR to submit one \(enrollment.vendor.title) key to your account.")
                                Text("The key will only be saved after you review and confirm it here.").font(.footnote).foregroundStyle(MKStyle.muted)
                                Button("Approve this computer") { claim(enrollment) }.fontWeight(.semibold).disabled(busy || errorMessage != nil)
                            } else if enrollment.status == "claimed" {
                                ProgressView("Waiting for your computer…")
                                Text("Create or copy your key in the provider’s console, then paste it on the computer’s MasterKey page. Keep this screen open.").font(.footnote).foregroundStyle(MKStyle.muted)
                                Button("Cancel key setup", role: .destructive) { cancel(enrollment) }.disabled(busy)
                            } else if enrollment.canConfirm {
                                Text("Review the submitted key").font(.headline)
                                LabeledContent("Provider", value: enrollment.vendor.title)
                                LabeledContent("New key ending", value: enrollment.last4 ?? "")
                                if let current = enrollment.currentLast4 {
                                    Label("This replaces your saved \(enrollment.vendor.title) key ending \(current).", systemImage: "exclamationmark.triangle").foregroundStyle(MKStyle.warning)
                                }
                                Text("Check that these last four characters match the key you pasted. The full key is never sent to this phone.").font(.footnote).foregroundStyle(MKStyle.muted)
                                Button(enrollment.currentLast4 == nil ? "Save this key" : "Replace saved key") { confirm(enrollment) }.fontWeight(.semibold).disabled(busy || errorMessage != nil)
                                Button("Cancel key setup", role: .destructive) { cancel(enrollment) }.disabled(busy)
                            } else {
                                Text("This setup is no longer available. Start again on your computer.")
                            }
                        }
                        if busy { Section { ProgressView("Updating setup…") } }
                    }
                }.scrollContentBackground(.hidden)
            }
        }.background(MKStyle.navy).navigationTitle("Add provider key").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } } }
            .task(id: loadID) { await loadAndPoll() }
            .onChange(of: store.contextGeneration) { _, _ in
                actionTask?.cancel(); viewToken = UUID(); enrollment = nil; saved = false; cancelled = false; busy = false; errorMessage = nil
            }
            .onDisappear { actionTask?.cancel(); viewToken = UUID() }
    }

    private func loadAndPoll() async {
        let token = UUID(); viewToken = token
        guard store.account != nil, !store.demoMode else { return }
        loading = true; errorMessage = nil; enrollment = nil
        defer { if viewToken == token { loading = false } }
        do {
            while !Task.isCancelled {
                let result = try await store.enrollment(id: id)
                guard viewToken == token, !Task.isCancelled, result.id == id else { return }
                enrollment = result
                guard result.shouldPoll else { return }
                try await Task.sleep(for: .seconds(2))
            }
        } catch {
            guard viewToken == token, !Task.isCancelled, !(error is CancellationError) else { return }
            errorMessage = error.localizedDescription
        }
    }
    private func continuePairing() {
        guard !busy else { return }
        let generation = store.contextGeneration
        busy = true
        actionTask = Task {
            defer { if generation == store.contextGeneration { busy = false } }
            await store.resumePairingAfterKeySetup()
        }
    }
    private func claim(_ preview: KeyEnrollment) {
        guard !busy, preview.id == id else { return }
        guard preview.canClaim else { errorMessage = "This setup has expired. Start again on your computer."; reload += 1; return }
        let generation = store.contextGeneration; let token = viewToken
        busy = true; errorMessage = nil
        actionTask = Task {
            defer { if generation == store.contextGeneration { busy = false } }
            do {
                let result = try await store.claimEnrollment(id: id)
                guard generation == store.contextGeneration, viewToken == token, !Task.isCancelled,
                      result.id == id, result.vendor == preview.vendor else { return }
                enrollment = result
                reload += 1
            } catch {
                if generation == store.contextGeneration, viewToken == token, !Task.isCancelled { errorMessage = "Could not confirm computer approval. Check setup status before trying again. " + error.localizedDescription }
            }
        }
    }
    private func confirm(_ candidate: KeyEnrollment) {
        guard !busy, candidate.id == id else { return }
        guard candidate.canConfirm else { errorMessage = "This setup has expired. Start again on your computer."; reload += 1; return }
        let generation = store.contextGeneration; let token = viewToken
        busy = true; errorMessage = nil
        actionTask = Task {
            defer { if generation == store.contextGeneration { busy = false } }
            do {
                try await store.confirmEnrollment(candidate)
                guard generation == store.contextGeneration, viewToken == token, !Task.isCancelled else { return }
                saved = true
                if store.pendingEnrollmentID == id { store.pendingEnrollmentID = nil }
            } catch {
                if generation == store.contextGeneration, viewToken == token, !Task.isCancelled { errorMessage = "Could not confirm that the key was saved. Check setup status before trying again. " + error.localizedDescription }
            }
        }
    }
    private func cancel(_ preview: KeyEnrollment) {
        guard !busy, preview.id == id, ["claimed", "submitted"].contains(preview.status) else { return }
        let generation = store.contextGeneration; let token = viewToken
        busy = true; errorMessage = nil
        actionTask = Task {
            defer { if generation == store.contextGeneration { busy = false } }
            do {
                try await store.cancelEnrollment(id: id)
                guard generation == store.contextGeneration, viewToken == token, !Task.isCancelled else { return }
                cancelled = true
            } catch {
                if generation == store.contextGeneration, viewToken == token, !Task.isCancelled { errorMessage = error.localizedDescription }
            }
        }
    }
}

private struct QRScannerView: UIViewControllerRepresentable {
    let onScan: (String) -> Void
    let onError: () -> Void
    func makeCoordinator() -> Coordinator { Coordinator(onScan: onScan, onError: onError) }
    func makeUIViewController(context: Context) -> DataScannerViewController {
        let controller = DataScannerViewController(recognizedDataTypes: [.barcode(symbologies: [.qr])], qualityLevel: .balanced, recognizesMultipleItems: false, isHighFrameRateTrackingEnabled: false, isHighlightingEnabled: true)
        controller.delegate = context.coordinator
        do { try controller.startScanning() } catch { DispatchQueue.main.async { onError() } }
        return controller
    }
    func updateUIViewController(_ controller: DataScannerViewController, context: Context) { }
    static func dismantleUIViewController(_ controller: DataScannerViewController, coordinator: Coordinator) { controller.stopScanning() }
    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let onScan: (String) -> Void
        let onError: () -> Void
        private var delivered = false
        init(onScan: @escaping (String) -> Void, onError: @escaping () -> Void) { self.onScan = onScan; self.onError = onError }
        func dataScanner(_ dataScanner: DataScannerViewController, didAdd addedItems: [RecognizedItem], allItems: [RecognizedItem]) {
            guard !delivered else { return }
            for item in addedItems {
                if case .barcode(let barcode) = item, let value = barcode.payloadStringValue {
                    delivered = true; dataScanner.stopScanning(); onScan(value); return
                }
            }
        }
        func dataScanner(_ dataScanner: DataScannerViewController, becameUnavailableWithError error: DataScannerViewController.ScanningUnavailable) { onError() }
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
