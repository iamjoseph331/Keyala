import SwiftUI
import AuthenticationServices
import VisionKit
import AVFoundation
import LocalAuthentication

struct PageHeader: View {
    let eyebrow: String
    let title: String
    let subtitle: String
    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            if !eyebrow.isEmpty { Text(eyebrow).font(.subheadline.weight(.medium)).foregroundStyle(MKStyle.teal) }
            Text(title).font(.system(size: 34, weight: .bold)).foregroundStyle(MKStyle.cream)
            Text(subtitle).font(.subheadline).foregroundStyle(MKStyle.muted)
        }.frame(maxWidth: .infinity, alignment: .leading).padding(.bottom, 10)
    }
}

struct Card<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View { content.padding(16).frame(maxWidth: .infinity, alignment: .leading).background(MKStyle.panel, in: RoundedRectangle(cornerRadius: 16, style: .continuous)).overlay(RoundedRectangle(cornerRadius: 16).stroke(MKStyle.line, lineWidth: 0.5)) }
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
    @AppStorage("allowSwipeOnce") private var allowSwipeOnce = false
    @State private var showHistory = false
    @State private var selected: RequestRecord?
    @State private var decidingIDs: Set<String> = []
    private var visible: [RequestRecord] { showHistory ? store.requests.filter { $0.status != "pending" } : store.requests.filter { $0.status == "pending" } }

    var body: some View {
        List {
            Section {
                ModeBanner()
                InfoBanner()
                Picker("Request list", selection: $showHistory) {
                    Text("Waiting").tag(false)
                    Text("Activity").tag(true)
                }.pickerStyle(.segmented)
            }.listRowBackground(Color.clear).listRowInsets(EdgeInsets(top: 4, leading: 0, bottom: 4, trailing: 0)).listRowSeparator(.hidden)
            if !showHistory {
                Section {
                    Button { NotificationCenter.default.post(name: .masterKeyConnectApp, object: nil) } label: {
                        Label("Scan to connect", systemImage: "qrcode.viewfinder").font(.headline).frame(maxWidth: .infinity).padding(.vertical, 6)
                    }.foregroundStyle(MKStyle.buttonText).listRowBackground(MKStyle.teal)
                }
            }
            if !showHistory && store.keys.isEmpty {
                Section {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("Add your first provider").font(.headline)
                        Text("Set up a provider once, then choose which apps can use it.").font(.subheadline).foregroundStyle(MKStyle.muted)
                        Button("Add a provider") { NotificationCenter.default.post(name: .masterKeyOpenKeys, object: nil) }.buttonStyle(.borderedProminent)
                    }.padding(.vertical, 8)
                }.listRowBackground(MKStyle.panel)
            } else if !showHistory && store.grants.isEmpty {
                Section {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Connect your first app").font(.headline)
                        Text("Scan the QR on an app’s website. You’ll review who is asking before allowing access.").font(.subheadline).foregroundStyle(MKStyle.muted)
                    }.padding(.vertical, 8)
                }.listRowBackground(MKStyle.panel)
            } else if visible.isEmpty {
                Section {
                    ContentUnavailableView(showHistory ? "No activity yet" : "You’re all caught up", systemImage: showHistory ? "clock" : "checkmark.circle", description: Text(showHistory ? "Past requests and responses will appear here." : "Requests that need your permission will appear here."))
                }.listRowBackground(Color.clear)
            }
            if !visible.isEmpty {
                Section(showHistory ? "Recent activity" : "\(visible.count) waiting for you") {
                    ForEach(visible) { request in
                        Button { selected = request } label: { RequestCard(request: request) }.buttonStyle(.plain)
                            .listRowBackground(MKStyle.panel)
                            .swipeActions(edge: .leading, allowsFullSwipe: false) {
                                if allowSwipeOnce && request.status == "pending" {
                                    if request.canApproveWithSwipe {
                                        Button { allowOnce(request) } label: { Label("Allow once", systemImage: "checkmark") }.tint(MKStyle.teal).disabled(decidingIDs.contains(request.id))
                                    } else {
                                        Button { selected = request } label: { Label("Review", systemImage: "doc.text.magnifyingglass") }.tint(MKStyle.teal)
                                    }
                                }
                            }
                    }
                }
            }
            if showHistory && !store.demoMode {
                Section {
                    if let error = store.olderRequestsError { Text(error).font(.footnote).foregroundStyle(MKStyle.warning) }
                    if store.isLoadingOlderRequests { ProgressView("Loading older requests…") }
                    else if store.hasOlderRequests {
                        Button(store.olderRequestsError == nil ? "Load older requests" : "Try again") { Task { await store.loadOlderRequests() } }
                    } else if !store.requests.isEmpty {
                        Text("You’ve reached the beginning of your activity.").font(.footnote).foregroundStyle(MKStyle.muted)
                    }
                }.listRowBackground(Color.clear)
            }
        }.listStyle(.insetGrouped).scrollContentBackground(.hidden).background(MKStyle.navy)
            .navigationTitle("Inbox").navigationBarTitleDisplayMode(.large)
            .refreshable { await store.refresh() }
            .onAppear { Task { await store.refresh() } }
            .sheet(item: $selected) { RequestDetailView(request: request(for: $0)) }
            .onChange(of: store.pendingRequestID) { _, _ in openNotificationRequest() }
            .onChange(of: store.requests.map(\.id)) { _, _ in openNotificationRequest() }
            .onChange(of: store.contextGeneration) { _, _ in selected = nil; decidingIDs = [] }
            .onAppear { openNotificationRequest() }
    }
    private func allowOnce(_ request: RequestRecord) {
        guard allowSwipeOnce, request.canApproveWithSwipe, !decidingIDs.contains(request.id) else { selected = request; return }
        decidingIDs.insert(request.id)
        Task { await store.decide(request, approve: true, policy: .once); decidingIDs.remove(request.id) }
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
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    let request: RequestRecord
    private var rowLayout: AnyLayout {
        dynamicTypeSize.isAccessibilitySize ? AnyLayout(VStackLayout(alignment: .leading, spacing: 12)) : AnyLayout(HStackLayout(alignment: .top, spacing: 12))
    }
    var body: some View {
        rowLayout {
            Text(String(request.appName.prefix(1)).uppercased()).font(.headline).foregroundStyle(MKStyle.teal)
                .frame(width: 38, height: 38).background(MKStyle.teal.opacity(0.10), in: RoundedRectangle(cornerRadius: 11))
            VStack(alignment: .leading, spacing: 5) {
                Text(request.appName).font(.headline).foregroundStyle(MKStyle.cream).lineLimit(2)
                Text(request.provider.title).font(.subheadline).foregroundStyle(MKStyle.muted)
                Text(request.status == "pending" ? "Tap to review what it sends" : request.createdAt.asDate).font(.caption).foregroundStyle(MKStyle.muted)
            }
            if !dynamicTypeSize.isAccessibilitySize { Spacer(minLength: 6) }
            VStack(alignment: dynamicTypeSize.isAccessibilitySize ? .leading : .trailing, spacing: 5) {
                Text(request.status == "completed" ? request.actualUSD : request.estimatedUSD).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(MKStyle.cream)
                Text(request.status == "completed" ? "Actual" : "Estimated").font(.caption2).foregroundStyle(MKStyle.muted)
                if request.status != "pending" { StatusPill(status: request.status) }
            }
        }.padding(.vertical, 8).contentShape(Rectangle())
    }
}

struct StatusPill: View {
    let status: String
    var color: Color { status == "pending" ? MKStyle.warning : (["completed", "approved"].contains(status) ? MKStyle.teal : MKStyle.muted) }
    var body: some View { Text(status == "pending" ? "Needs review" : status.capitalized).font(.caption2.weight(.semibold)).foregroundStyle(color).padding(.horizontal, 8).padding(.vertical, 4).background(color.opacity(0.10), in: Capsule()) }
}

struct RequestDetailView: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    @AppStorage("requireDeviceAuthentication") private var requireDeviceAuthentication = false
    @ScaledMetric(relativeTo: .largeTitle) private var priceSize = 46
    @State private var showPolicy = false
    @State private var loadingDetail = false
    @State private var deciding = false
    @State private var responseExpanded = true
    @State private var access = Policy.once
    let request: RequestRecord
    private var current: RequestRecord { store.requests.first(where: { $0.id == request.id }) ?? request }
    private var decisionLayout: AnyLayout {
        dynamicTypeSize.isAccessibilitySize ? AnyLayout(VStackLayout(spacing: 10)) : AnyLayout(HStackLayout(spacing: 12))
    }
    private var accessTitle: String {
        switch access.mode {
        case "count": "This and the next calls"
        case "budget": "Within a budget"
        case "trust": "Trust this app"
        default: "This call only"
        }
    }
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    VStack(spacing: 10) {
                        Text(String(current.appName.prefix(1)).uppercased())
                            .font(.title.bold()).foregroundStyle(MKStyle.teal)
                            .frame(width: 60, height: 60).background(MKStyle.teal.opacity(0.10), in: RoundedRectangle(cornerRadius: 17))
                        Text(current.appName).font(.title2.bold()).multilineTextAlignment(.center)
                        Text("\(current.provider.title) / \(current.model)").font(.subheadline).foregroundStyle(MKStyle.muted).multilineTextAlignment(.center)
                        StatusPill(status: current.status)
                        Text(current.status == "completed" ? current.actualCostMicrousd.map { String(format: "$%.6f", Double($0) / 1_000_000) } ?? "Not reported" : current.estimatedUSD)
                            .font(.system(size: priceSize, weight: .semibold)).monospacedDigit().minimumScaleFactor(0.6).lineLimit(1)
                        Text(current.status == "completed" ? "Cost from reported usage" : "Estimated cost")
                            .font(.subheadline).foregroundStyle(MKStyle.muted)
                    }.frame(maxWidth: .infinity).padding(.top, 12)
                    if current.isDemo { ModeBanner() }
                    if current.status == "uncertain" {
                        Label("The provider may have processed and charged for this request. It will not be retried automatically.", systemImage: "exclamationmark.triangle")
                            .font(.footnote).foregroundStyle(MKStyle.warning)
                    }
                    if current.status == "sending" { ProgressView("Waiting for the provider…") }
                    Card {
                        VStack(alignment: .leading, spacing: 16) {
                            Text("What it sends").font(.headline)
                            if loadingDetail { ProgressView("Loading the complete request…") }
                            if current.payload == nil {
                                Text("The complete request must load before you can allow it.").font(.subheadline).foregroundStyle(MKStyle.muted)
                            } else {
                                ForEach(current.readableMessages) { message in
                                    VStack(alignment: .leading, spacing: 6) {
                                        Text(message.role).font(.caption.weight(.semibold)).foregroundStyle(MKStyle.muted)
                                        Text(message.text).font(.body).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                                    }
                                }
                                DisclosureGroup("Full request") { json(current.payload!) }
                            }
                        }
                    }
                    Text("The estimate reserves an allowance; it is not a guarantee of the provider’s final charge.")
                        .font(.footnote).foregroundStyle(MKStyle.muted)
                    if current.status == "pending" {
                        Card {
                            Button { showPolicy = true } label: {
                                HStack { Text("Allow").foregroundStyle(MKStyle.muted); Spacer(); Text(accessTitle); Image(systemName: "chevron.down").font(.caption) }
                            }.buttonStyle(.plain).disabled(deciding)
                        }

                    } else {
                        if let response = current.response {
                            Card { DisclosureGroup("Provider response", isExpanded: $responseExpanded) { json(response) } }
                        } else if !loadingDetail {
                            Text(current.error ?? "No provider response was recorded.").font(.subheadline).foregroundStyle(MKStyle.muted)
                        }
                    }
                }.padding(20)
            }.safeAreaInset(edge: .bottom) {
                if current.status == "pending" {
                    VStack(spacing: 8) {
                        decisionLayout {
                            Button(role: .destructive) { decide(approve: false) } label: { Text("Deny").lineLimit(1).frame(maxWidth: .infinity) }.buttonStyle(.bordered)
                            Button { decide(approve: true) } label: {
                                HStack { if deciding { ProgressView() }; Text("Allow").lineLimit(1); if requireDeviceAuthentication { Image(systemName: "lock.shield") } }.frame(maxWidth: .infinity)
                            }.buttonStyle(.borderedProminent).tint(MKStyle.teal).foregroundStyle(MKStyle.buttonText)
                                .disabled(loadingDetail || current.payload == nil)
                        }.controlSize(.large).disabled(deciding)
                        if requireDeviceAuthentication { Text("Confirm with Face ID, Touch ID or your device passcode.").font(.footnote).foregroundStyle(MKStyle.muted) }
                    }.padding(.horizontal, 20).padding(.vertical, 12).background(MKStyle.navy)
                }
            }.background(MKStyle.navy).navigationTitle("Review request").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .topBarTrailing) { Button("Done") { dismiss() } } }
                .sheet(isPresented: $showPolicy) {
                    ApprovalPolicyView(actionTitle: "Use this access") { policy in access = policy; showPolicy = false }
                }
                .task(id: "\(request.id):\(current.status)") { loadingDetail = true; await store.loadRequestDetails(id: current.id); loadingDetail = false }
                .refreshable { await store.loadRequestDetails(id: current.id) }
        }.presentationDetents([.large])
    }
    private func json(_ value: JSONValue) -> some View {
        ScrollView(.horizontal) { Text(value.prettyPrinted).font(.system(.footnote, design: .monospaced)).textSelection(.enabled).padding(.vertical, 10) }
    }
    private func decide(approve: Bool) {
        guard !deciding else { return }
        deciding = true
        Task {
            defer { deciding = false }
            await store.decide(current, approve: approve, policy: approve ? access : .once)
            if !approve && current.status == "denied" { dismiss() }
        }
    }
}

struct ApprovalPolicyView: View {
    @Environment(\.dismiss) private var dismiss
    @State private var mode = "ask"
    @State private var count = 5
    @State private var budget = 1.0
    @State private var alert = 0.5
    var actionTitle = "Apply access"
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
                Section { Text("You can change or remove app access any time. Estimates are not guaranteed final charges.").font(.footnote).foregroundStyle(MKStyle.muted) }
                Section {
                    Button(actionTitle) { onApprove(Policy(mode: mode, remainingCount: mode == "count" ? count : nil, remainingUSD: mode == "budget" ? budget : nil, alertUSDPerDay: mode == "trust" ? alert : nil)) }
                        .fontWeight(.semibold)
                        .frame(maxWidth: .infinity)
                        .buttonStyle(.borderedProminent)
                        .tint(MKStyle.teal)
                        .foregroundStyle(MKStyle.navy)
                }
            }.scrollContentBackground(.hidden).background(MKStyle.navy).navigationTitle("Access for this app").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
        }.presentationDetents([.medium, .large])
    }
}

struct VaultView: View {
    var returnConnectCode: String? = nil
    var suggestedProvider: Provider? = nil
    @EnvironmentObject private var store: AppStore
    @State private var showPaste = false
    @State private var selectedKey: KeyRecord?
    @State private var confirmRouterReplacement = false
    private var hasOpenRouter: Bool { store.keys.contains(where: { $0.provider == .openRouter }) }
    var body: some View {
        List {
            Section { ModeBanner(); InfoBanner() }.listRowBackground(Color.clear).listRowSeparator(.hidden)
            if !store.keys.isEmpty {
                Section("Connected providers") {
                    ForEach(store.keys) { key in
                        Button { selectedKey = key } label: {
                            HStack(spacing: 12) {
                                Image(systemName: "key.horizontal").font(.title3).foregroundStyle(MKStyle.teal)
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(key.provider.title).font(.headline).foregroundStyle(MKStyle.cream)
                                    Text("Key ending \(key.lastFour)").font(.subheadline).foregroundStyle(MKStyle.muted)
                                }
                                Spacer(); Image(systemName: "chevron.right").font(.caption).foregroundStyle(MKStyle.muted)
                            }.padding(.vertical, 5)
                        }
                    }
                }.listRowBackground(MKStyle.panel)
            }
            Section(hasOpenRouter ? "OpenRouter connection" : "Add a provider") {
                VStack(alignment: .leading, spacing: 14) {
                    Label("OpenRouter", systemImage: "link").font(.headline)
                    Text("Connect your OpenRouter account without copying a key. Requests use your OpenRouter balance.").font(.subheadline).foregroundStyle(MKStyle.muted)
                    Text("A $10 spending cap is suggested on the consent page. You can change it there.").font(.footnote).foregroundStyle(MKStyle.muted)
                    Button {
                        if hasOpenRouter { confirmRouterReplacement = true }
                        else { Task { await store.connectOpenRouter() } }
                    } label: {
                        HStack { if store.isConnectingOpenRouter { ProgressView() }; Text(store.isConnectingOpenRouter ? "Connecting…" : (hasOpenRouter ? "Reconnect OpenRouter" : "Connect OpenRouter")).frame(maxWidth: .infinity) }
                    }.buttonStyle(.borderedProminent).tint(MKStyle.teal).foregroundStyle(MKStyle.buttonText)
                        .disabled(store.demoMode || store.isConnectingOpenRouter)
                }.padding(.vertical, 8)
            }.listRowBackground(MKStyle.panel)
            Section("OpenAI, Anthropic or an existing key") {
                Button { NotificationCenter.default.post(name: .masterKeyAddFromComputer, object: returnConnectCode) } label: {
                    Label { VStack(alignment: .leading, spacing: 4) {
                        Text("Paste from your computer").foregroundStyle(MKStyle.cream)
                        Text("Scan the setup QR, then confirm on this phone.").font(.footnote).foregroundStyle(MKStyle.muted)
                    } } icon: { Image(systemName: "desktopcomputer") }
                }.disabled(store.demoMode)
                Button { showPaste = true } label: { Label("Paste a key on this iPhone", systemImage: "doc.on.clipboard").foregroundStyle(MKStyle.cream) }
                    .disabled(store.demoMode)
            }.listRowBackground(MKStyle.panel)
            Section { Text("Apps receive permission to use a provider. They never receive your key.").font(.footnote).foregroundStyle(MKStyle.muted) }.listRowBackground(Color.clear)
        }.listStyle(.insetGrouped).scrollContentBackground(.hidden).background(MKStyle.navy)
            .navigationTitle("Keys").navigationBarTitleDisplayMode(.large)
            .alert("Replace your OpenRouter key?", isPresented: $confirmRouterReplacement) {
                Button("Continue") { Task { await store.connectOpenRouter() } }
                Button("Keep current key", role: .cancel) { }
            } message: { Text("Completing authorization replaces the saved OpenRouter key. Connected apps will use the new key under their existing permissions.") }
            .sheet(isPresented: $showPaste) { PasteKeyView(provider: suggestedProvider ?? .openAI) }
            .sheet(item: $selectedKey) { key in ProviderKeyView(key: key) }
            .onChange(of: store.contextGeneration) { _, _ in selectedKey = nil; showPaste = false }
    }
}

private struct PasteKeyView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    @State private var provider: Provider
    @State private var secret = ""
    @State private var saving = false
    init(provider: Provider) { _provider = State(initialValue: provider) }
    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker("Provider", selection: $provider) { ForEach(Provider.allCases) { Text($0.title).tag($0) } }.disabled(saving)
                    Link(destination: provider.keysURL) { Label("Create a key at \(provider.title)", systemImage: "arrow.up.right.square") }
                    Text("Copy a key from the provider, then paste it here. Universal Clipboard can bring a key copied on your Mac to this iPhone.").font(.footnote).foregroundStyle(MKStyle.muted)
                    HStack {
                        SecureField(provider.keyPlaceholder, text: $secret).textInputAutocapitalization(.never).autocorrectionDisabled().privacySensitive().disabled(saving)
                        PasteButton(payloadType: String.self) { values in
                            if let value = values.first { secret = value.trimmingCharacters(in: .whitespacesAndNewlines) }
                        }.labelStyle(.iconOnly).disabled(saving || store.demoMode)
                    }
                }
                Section {
                    Button {
                        let value = secret; let selected = provider; let generation = store.contextGeneration
                        saving = true
                        Task {
                            defer { saving = false }
                            if await store.addKey(provider: selected, value: value, label: selected.title), generation == store.contextGeneration {
                                secret = ""; dismiss()
                            }
                        }
                    } label: { HStack { if saving { ProgressView() }; Text(saving ? "Saving…" : "Save key").frame(maxWidth: .infinity) } }
                        .buttonStyle(.borderedProminent).foregroundStyle(MKStyle.buttonText)
                        .disabled(store.demoMode || saving || secret.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    Text("Sent to your MasterKey server over HTTPS and encrypted there. Saving does not make a billable model call.").font(.footnote).foregroundStyle(MKStyle.muted)
                }
            }.scrollContentBackground(.hidden).background(MKStyle.navy).navigationTitle("Paste provider key").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
                .onChange(of: provider) { _, _ in secret = "" }
                .onChange(of: store.contextGeneration) { _, _ in secret = ""; dismiss() }
                .onDisappear { secret = "" }
        }
    }
}

private struct ProviderKeyView: View {
    let key: KeyRecord
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    @State private var confirmRemoval = false
    var body: some View {
        NavigationStack {
            Form {
                Section {
                    LabeledContent("Provider", value: key.provider.title)
                    LabeledContent("Key ending", value: key.lastFour)
                    if let created = key.createdAt { LabeledContent("Added", value: created.asDate) }
                    Text("The full key cannot be viewed or exported.").font(.footnote).foregroundStyle(MKStyle.muted)
                }
                Section("Connected apps") {
                    let apps = store.grants.filter { $0.provider == key.provider }
                    if apps.isEmpty { Text("No apps use this provider yet.").foregroundStyle(MKStyle.muted) }
                    ForEach(apps) { app in Text(app.appName) }
                }
                Section { Button("Remove key", role: .destructive) { confirmRemoval = true } }
            }.scrollContentBackground(.hidden).background(MKStyle.navy).navigationTitle(key.provider.title)
                .toolbar { ToolbarItem(placement: .topBarTrailing) { Button("Done") { dismiss() } } }
                .confirmationDialog("Remove this key? Apps using it will stop working until you add another.", isPresented: $confirmRemoval, titleVisibility: .visible) {
                    Button("Remove key", role: .destructive) { Task { await store.removeKey(key); if !store.keys.contains(where: { $0.provider == key.provider }) { dismiss() } } }
                    Button("Keep key", role: .cancel) { }
                }
        }
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
            Text("Tap an app to change its access or disconnect it.").font(.caption).foregroundStyle(MKStyle.muted)
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
            Image("BrandMark").resizable().scaledToFit().frame(width: 88, height: 88).clipShape(RoundedRectangle(cornerRadius: 22)).accessibilityHidden(true).padding(.top, 28)
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
    @AppStorage("allowSwipeOnce") private var allowSwipeOnce = false
    @AppStorage("requireDeviceAuthentication") private var requireDeviceAuthentication = false
    @State private var showSwipeWarning = false
    @State private var canAuthenticate = true
    var body: some View {
        Form {
            Section("Account") { NavigationLink { AccountView() } label: { Label(store.account == nil ? "Sign in" : "Account", systemImage: "person.crop.circle") } }
            Section("Requests") {
                Button { Task { await store.requestNotifications() } } label: { Label("Notification permission", systemImage: "bell") }
                Toggle("Verify approvals on this device", isOn: $requireDeviceAuthentication).disabled(!canAuthenticate && !requireDeviceAuthentication)
                Text(canAuthenticate ? "Use Face ID, Touch ID or your device passcode before allowing a request." : "Set up a device passcode or biometrics in iPhone Settings to enable approval verification.").font(.footnote).foregroundStyle(MKStyle.muted)
                Toggle("Swipe to allow", isOn: Binding(get: { allowSwipeOnce }, set: { value in
                    if value { showSwipeWarning = true } else { allowSwipeOnce = false }
                }))
                Text("Off by default. Only one-call approvals with an estimate of $0.05 or less can use this shortcut. Larger estimates open the review sheet.").font(.footnote).foregroundStyle(MKStyle.muted)
            }
            Section("Appearance") { AppearancePicker() }
            Section("Developer") { NavigationLink("Developer options") { DeveloperOptionsView() } }
            Section { InfoBanner() }
        }.scrollContentBackground(.hidden).background(MKStyle.navy).navigationTitle("Settings")
            .task { var error: NSError?; canAuthenticate = LAContext().canEvaluatePolicy(.deviceOwnerAuthentication, error: &error) }
            .alert("Allow requests with a swipe?", isPresented: $showSwipeWarning) {
                Button("Turn on", role: .destructive) { allowSwipeOnce = true }
                Button("Keep off", role: .cancel) { }
            } message: {
                Text("This can skip the review sheet. Someone holding your unlocked phone could approve spending when device verification is off. Only a loaded request with an estimate of $0.05 or less can be allowed, for this call only. Estimates are not guaranteed final charges.")
            }
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
                                NavigationLink("Add your \(preview.vendor.title) key first") { VaultView(returnConnectCode: pairing.code, suggestedProvider: preview.vendor) }
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
