import SwiftUI
import Combine
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
            if !eyebrow.isEmpty { Text(LocalizedStringKey(eyebrow)).font(.subheadline.weight(.medium)).foregroundStyle(KeyalaStyle.teal) }
            Text(LocalizedStringKey(title)).font(.system(size: 34, weight: .bold)).foregroundStyle(KeyalaStyle.cream)
            Text(LocalizedStringKey(subtitle)).font(.subheadline).foregroundStyle(KeyalaStyle.muted)
        }.frame(maxWidth: .infinity, alignment: .leading).padding(.bottom, 10)
    }
}

struct Card<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View { content.padding(16).frame(maxWidth: .infinity, alignment: .leading).background(KeyalaStyle.panel, in: RoundedRectangle(cornerRadius: 16, style: .continuous)).overlay(RoundedRectangle(cornerRadius: 16).stroke(KeyalaStyle.line, lineWidth: 0.5)) }
}

struct InfoBanner: View {
    @EnvironmentObject private var store: AppStore
    var body: some View {
        if let message = store.infoMessage {
            Button { store.infoMessage = nil } label: {
                HStack(alignment: .top, spacing: 10) { Image(systemName: "info.circle.fill").foregroundStyle(KeyalaStyle.teal); Text(message).font(.footnote).foregroundStyle(KeyalaStyle.cream); Spacer() }.padding(12).background(KeyalaStyle.teal.opacity(0.12), in: RoundedRectangle(cornerRadius: 14))
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
                    Button { NotificationCenter.default.post(name: .keyalaConnectApp, object: nil) } label: {
                        Label("Scan Keyala QR", systemImage: "qrcode.viewfinder").font(.headline).frame(maxWidth: .infinity).padding(.vertical, 6)
                    }.foregroundStyle(KeyalaStyle.buttonText).listRowBackground(KeyalaStyle.teal)
                }
            }
            if !showHistory && store.keys.isEmpty {
                Section {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("Add your first provider").font(.headline)
                        Text("Set up a provider once, then choose which apps can use it.").font(.subheadline).foregroundStyle(KeyalaStyle.muted)
                        Button("Add a provider") { NotificationCenter.default.post(name: .keyalaOpenKeys, object: nil) }.buttonStyle(.borderedProminent)
                    }.padding(.vertical, 8)
                }.listRowBackground(KeyalaStyle.panel)
            } else if !showHistory && store.grants.isEmpty {
                Section {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Connect your first app").font(.headline)
                        Text("Scan the QR on an app’s website. You’ll review who is asking before allowing access.").font(.subheadline).foregroundStyle(KeyalaStyle.muted)
                    }.padding(.vertical, 8)
                }.listRowBackground(KeyalaStyle.panel)
            } else if visible.isEmpty {
                Section {
                    ContentUnavailableView(showHistory ? "No activity yet" : "You’re all caught up", systemImage: showHistory ? "clock" : "checkmark.circle", description: Text(showHistory ? "Past requests and responses will appear here." : "Requests that need your permission will appear here."))
                }.listRowBackground(Color.clear)
            }
            if !visible.isEmpty {
                Section(showHistory ? "Recent activity" : "\(visible.count) waiting for you") {
                    ForEach(visible) { request in
                        Button { selected = request } label: { RequestCard(request: request) }.buttonStyle(.plain)
                            .listRowBackground(KeyalaStyle.panel)
                            .swipeActions(edge: .leading, allowsFullSwipe: false) {
                                if allowSwipeOnce && request.status == "pending" {
                                    if request.canApproveWithSwipe {
                                        Button { allowOnce(request) } label: { Label("Allow once", systemImage: "checkmark") }.tint(KeyalaStyle.teal).disabled(decidingIDs.contains(request.id))
                                    } else {
                                        Button { selected = request } label: { Label("Review", systemImage: "doc.text.magnifyingglass") }.tint(KeyalaStyle.teal)
                                    }
                                }
                            }
                    }
                }
            }
            if showHistory && !store.demoMode {
                Section {
                    if let error = store.olderRequestsError { Text(error).font(.footnote).foregroundStyle(KeyalaStyle.warning) }
                    if store.isLoadingOlderRequests { ProgressView("Loading older requests…") }
                    else if store.hasOlderRequests {
                        Button(store.olderRequestsError == nil ? "Load older requests" : "Try again") { Task { await store.loadOlderRequests() } }
                    } else if !store.requests.isEmpty {
                        Text("You’ve reached the beginning of your activity.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                    }
                }.listRowBackground(Color.clear)
            }
        }.listStyle(.insetGrouped).scrollContentBackground(.hidden).background(KeyalaStyle.navy)
            .localizedNavigationTitle("Inbox").navigationBarTitleDisplayMode(.large)
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
                .font(.footnote).foregroundStyle(KeyalaStyle.warning).padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(KeyalaStyle.warning.opacity(0.1), in: RoundedRectangle(cornerRadius: 12))
        } else if store.serverURL != AppPreferences.productionURL {
            Label("Development server", systemImage: "wrench.and.screwdriver")
                .font(.caption).foregroundStyle(KeyalaStyle.warning)
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
            Text(String(request.appName.prefix(1)).uppercased()).font(.headline).foregroundStyle(KeyalaStyle.teal)
                .frame(width: 38, height: 38).background(KeyalaStyle.teal.opacity(0.10), in: RoundedRectangle(cornerRadius: 11))
            VStack(alignment: .leading, spacing: 5) {
                Text(request.appName).font(.headline).foregroundStyle(KeyalaStyle.cream).lineLimit(2)
                Text(request.provider.title).font(.subheadline).foregroundStyle(KeyalaStyle.muted)
                Text(request.status == "pending" ? "Tap to review what it sends" : request.createdAt.asDate).font(.caption).foregroundStyle(KeyalaStyle.muted)
            }
            if !dynamicTypeSize.isAccessibilitySize { Spacer(minLength: 6) }
            VStack(alignment: dynamicTypeSize.isAccessibilitySize ? .leading : .trailing, spacing: 5) {
                Text(request.status == "completed" ? request.actualUSD : request.estimatedUSD).font(.subheadline.weight(.semibold)).monospacedDigit().foregroundStyle(KeyalaStyle.cream)
                Text(request.status == "completed" ? "Actual" : "Estimated").font(.caption2).foregroundStyle(KeyalaStyle.muted)
                if request.status != "pending" { StatusPill(status: request.status) }
            }
        }.padding(.vertical, 8).contentShape(Rectangle())
    }
}

struct StatusPill: View {
    let status: String
    var color: Color { status == "pending" ? KeyalaStyle.warning : (["completed", "approved"].contains(status) ? KeyalaStyle.teal : KeyalaStyle.muted) }
    var body: some View { Text(LocalizedStringKey(status == "pending" ? "Needs review" : status.capitalized)).font(.caption2.weight(.semibold)).foregroundStyle(color).padding(.horizontal, 8).padding(.vertical, 4).background(color.opacity(0.10), in: Capsule()) }
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
                            .font(.title.bold()).foregroundStyle(KeyalaStyle.teal)
                            .frame(width: 60, height: 60).background(KeyalaStyle.teal.opacity(0.10), in: RoundedRectangle(cornerRadius: 17))
                        Text(current.appName).font(.title2.bold()).multilineTextAlignment(.center)
                        Text("\(current.provider.title) / \(current.model)").font(.subheadline).foregroundStyle(KeyalaStyle.muted).multilineTextAlignment(.center)
                        if current.coupon == nil, current.billingVendor == .openRouter && current.provider != .openRouter {
                            Label("Billed through OpenRouter credits", systemImage: "arrow.triangle.branch").font(.subheadline).foregroundStyle(KeyalaStyle.teal)
                            if let model = current.upstreamModel { Text(model).font(.caption).textSelection(.enabled) }
                        }
                        StatusPill(status: current.status)
                        Text(current.status == "completed" ? current.actualCostMicrousd.map { String(format: "$%.6f", Double($0) / 1_000_000) } ?? "Not reported" : current.estimatedUSD)
                            .font(.system(size: priceSize, weight: .semibold)).monospacedDigit().minimumScaleFactor(0.6).lineLimit(1)
                        Text(current.status == "completed" ? "Cost from reported usage" : "Estimated cost")
                            .font(.subheadline).foregroundStyle(KeyalaStyle.muted)
                    }.frame(maxWidth: .infinity).padding(.top, 12)
                    if current.isDemo { ModeBanner() }
                    if let coupon = current.coupon {
                        Card { CouponDetails(coupon: coupon) }
                    }
                    if current.status == "uncertain" {
                        Label("The provider may have processed and charged for this request. It will not be retried automatically.", systemImage: "exclamationmark.triangle")
                            .font(.footnote).foregroundStyle(KeyalaStyle.warning)
                    }
                    if current.status == "sending" { ProgressView("Waiting for the provider…") }
                    Card {
                        VStack(alignment: .leading, spacing: 16) {
                            Text("What it sends").font(.headline)
                            if loadingDetail { ProgressView("Loading the complete request…") }
                            if current.payload == nil {
                                Text("The complete request must load before you can allow it.").font(.subheadline).foregroundStyle(KeyalaStyle.muted)
                            } else {
                                ForEach(current.readableMessages) { message in
                                    VStack(alignment: .leading, spacing: 6) {
                                        Text(message.role).font(.caption.weight(.semibold)).foregroundStyle(KeyalaStyle.muted)
                                        Text(message.text).font(.body).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                                    }
                                }
                                if let payload = current.payload {
                                    if current.coupon != nil {
                                        Text("Full request").font(.headline)
                                        json(payload)
                                    } else {
                                        DisclosureGroup("Full request") { json(payload) }
                                    }
                                }
                            }
                        }
                    }
                    Text("The estimate reserves an allowance; it is not a guarantee of the provider’s final charge.")
                        .font(.footnote).foregroundStyle(KeyalaStyle.muted)
                    if current.status == "pending" {
                        Card {
                            if current.coupon != nil {
                                Label("This call only", systemImage: "hand.raised")
                            } else {
                                Button { showPolicy = true } label: {
                                    HStack { Text("Allow").foregroundStyle(KeyalaStyle.muted); Spacer(); Text(LocalizedStringKey(accessTitle)); Image(systemName: "chevron.down").font(.caption) }
                                }.buttonStyle(.plain).disabled(deciding)
                            }
                        }

                    } else {
                        if let response = current.response {
                            Card { DisclosureGroup("Provider response", isExpanded: $responseExpanded) { json(response) } }
                        } else if !loadingDetail {
                            Text(LocalizedStringKey(APIError.couponMessage(for: current.error) ?? current.error ?? "No provider response was recorded.")).font(.subheadline).foregroundStyle(KeyalaStyle.muted)
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
                            }.buttonStyle(.borderedProminent).tint(KeyalaStyle.teal).foregroundStyle(KeyalaStyle.buttonText)
                                .disabled(loadingDetail || current.payload == nil || current.coupon.map { $0.status != .active } == true)
                        }.controlSize(.large).disabled(deciding)
                        if requireDeviceAuthentication { Text("Confirm with Face ID, Touch ID or your device passcode.").font(.footnote).foregroundStyle(KeyalaStyle.muted) }
                    }.padding(.horizontal, 20).padding(.vertical, 12).background(KeyalaStyle.navy)
                }
            }.background(KeyalaStyle.navy).localizedNavigationTitle("Review request").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .topBarTrailing) { Button("Done") { dismiss() } } }
                .sheet(isPresented: $showPolicy) {
                    ApprovalPolicyView(actionTitle: "Use this access") { policy in access = policy; showPolicy = false }
                }
                .onChange(of: current.coupon?.id) { _, id in if id != nil { showPolicy = false } }
                .onChange(of: store.contextGeneration) { _, _ in dismiss() }
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
            await store.decide(current, approve: approve, policy: approve ? access : .once, reviewed: true)
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
    var isApplying = false
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
                Section { Text("You can change or remove app access any time. Estimates are not guaranteed final charges.").font(.footnote).foregroundStyle(KeyalaStyle.muted) }
                Section {
                    Button(actionTitle) { onApprove(Policy(mode: mode, remainingCount: mode == "count" ? count : nil, remainingUSD: mode == "budget" ? budget : nil, alertUSDPerDay: mode == "trust" ? alert : nil)) }
                        .disabled(isApplying)
                        .fontWeight(.semibold)
                        .frame(maxWidth: .infinity)
                        .buttonStyle(.borderedProminent)
                        .tint(KeyalaStyle.teal)
                        .foregroundStyle(KeyalaStyle.navy)
                }
            }.scrollContentBackground(.hidden).background(KeyalaStyle.navy).localizedNavigationTitle("Access for this app").navigationBarTitleDisplayMode(.inline)
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
                                Image(systemName: "key.horizontal").font(.title3).foregroundStyle(KeyalaStyle.teal)
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(key.provider.title).font(.headline).foregroundStyle(KeyalaStyle.cream)
                                    Text("Key ending \(key.lastFour)").font(.subheadline).foregroundStyle(KeyalaStyle.muted)
                                }
                                Spacer(); Image(systemName: "chevron.right").font(.caption).foregroundStyle(KeyalaStyle.muted)
                            }.padding(.vertical, 5)
                        }
                    }
                }.listRowBackground(KeyalaStyle.panel)
            }
            Section(hasOpenRouter ? "OpenRouter connection" : "Add a provider") {
                VStack(alignment: .leading, spacing: 14) {
                    HStack { Label("OpenRouter", systemImage: "link").font(.headline); Spacer(); if hasOpenRouter { Label("Connected", systemImage: "checkmark.circle.fill").font(.subheadline).foregroundStyle(KeyalaStyle.muted) } }
                    Text("Connect your OpenRouter account without copying a key. Requests use your OpenRouter balance.").font(.subheadline).foregroundStyle(KeyalaStyle.muted)
                    Text("A $10 spending cap is suggested on the consent page. You can change it there.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                    Button {
                        if hasOpenRouter { confirmRouterReplacement = true }
                        else { Task { await store.connectOpenRouter() } }
                    } label: {
                        HStack { if store.isConnectingOpenRouter { ProgressView() }; Text(store.isConnectingOpenRouter ? "Connecting…" : (hasOpenRouter ? "Reconnect OpenRouter" : "Connect OpenRouter")).frame(maxWidth: .infinity) }
                    }.buttonStyle(.borderedProminent).tint(hasOpenRouter ? KeyalaStyle.muted : KeyalaStyle.teal).foregroundStyle(KeyalaStyle.buttonText)
                        .disabled(store.demoMode || store.isConnectingOpenRouter)
                }.padding(.vertical, 8)
            }.listRowBackground(KeyalaStyle.panel)
            Section("OpenAI, Anthropic or an existing key") {
                Button { NotificationCenter.default.post(name: .keyalaAddFromComputer, object: returnConnectCode) } label: {
                    Label { VStack(alignment: .leading, spacing: 4) {
                        Text("Paste from your computer").foregroundStyle(KeyalaStyle.cream)
                        Text("Scan the setup QR, then confirm on this phone.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                    } } icon: { Image(systemName: "desktopcomputer") }
                }.disabled(store.demoMode)
                Button { showPaste = true } label: { Label("Paste a key on this iPhone", systemImage: "doc.on.clipboard").foregroundStyle(KeyalaStyle.cream) }
                    .disabled(store.demoMode)
            }.listRowBackground(KeyalaStyle.panel)
            Section { Text("Apps receive permission to use a provider. They never receive your key.").font(.footnote).foregroundStyle(KeyalaStyle.muted) }.listRowBackground(Color.clear)
        }.listStyle(.insetGrouped).scrollContentBackground(.hidden).background(KeyalaStyle.navy)
            .localizedNavigationTitle("Keys").navigationBarTitleDisplayMode(.large)
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
    @State private var confirmReplacement = false
    init(provider: Provider) { _provider = State(initialValue: provider) }
    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker("Provider", selection: $provider) { ForEach(Provider.allCases) { Text($0.title).tag($0) } }.disabled(saving)
                    Link(destination: provider.keysURL) { Label("Create a key at \(provider.title)", systemImage: "arrow.up.right.square") }
                    Text("Copy a key from the provider, then paste it here. Universal Clipboard can bring a key copied on your Mac to this iPhone.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                    HStack {
                        SecureField(provider.keyPlaceholder, text: $secret).textInputAutocapitalization(.never).autocorrectionDisabled().privacySensitive().disabled(saving)
                        PasteButton(payloadType: String.self) { values in
                            if let value = values.first { secret = value.trimmingCharacters(in: .whitespacesAndNewlines) }
                        }.labelStyle(.iconOnly).disabled(saving || store.demoMode)
                    }
                }
                Section {
                    Button {
                        if store.keys.contains(where: { $0.provider == provider }) { confirmReplacement = true }
                        else { saveKey() }
                    } label: { HStack { if saving { ProgressView() }; Text(saving ? "Saving…" : "Save key").frame(maxWidth: .infinity) } }
                        .buttonStyle(.borderedProminent).foregroundStyle(KeyalaStyle.buttonText)
                        .disabled(store.demoMode || saving || secret.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    Text("Sent to your Keyala server over HTTPS and encrypted there. Saving does not make a billable model call.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                }
            }.scrollContentBackground(.hidden).background(KeyalaStyle.navy).localizedNavigationTitle("Paste provider key").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
                .confirmationDialog("Replace the saved key?", isPresented: $confirmReplacement, titleVisibility: .visible) {
                    Button("Replace key", role: .destructive) { saveKey() }
                    Button("Keep current key", role: .cancel) { }
                } message: { Text("Connected apps will use the new key under their existing permissions. The previous key is not revoked at the provider.") }
                .onChange(of: provider) { _, _ in secret = "" }
                .onChange(of: store.contextGeneration) { _, _ in secret = ""; dismiss() }
                .onDisappear { secret = "" }
        }
    }
    private func saveKey() {
        guard !saving, !secret.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        let value = secret; let selected = provider; let generation = store.contextGeneration
        saving = true
        Task {
            defer { saving = false }
            if await store.addKey(provider: selected, value: value, label: selected.title), generation == store.contextGeneration {
                secret = ""; dismiss()
            }
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
                    Text("The full key cannot be viewed or exported.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                }
                Section("Connected apps") {
                    let apps = store.grants.filter { $0.provider == key.provider }
                    if apps.isEmpty { Text("No apps use this provider yet.").foregroundStyle(KeyalaStyle.muted) }
                    ForEach(apps) { app in Text(app.appName) }
                }
                Section { Button("Remove key", role: .destructive) { confirmRemoval = true } }
            }.scrollContentBackground(.hidden).background(KeyalaStyle.navy).localizedNavigationTitle(key.provider.title)
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
            NavigationLink { CouponsView() } label: {
                Card { Label("Coupons", systemImage: "ticket").frame(maxWidth: .infinity, alignment: .leading) }
            }.buttonStyle(.plain)
            if store.grants.isEmpty { Card { Text("Connect an app using the QR code on its website. You’ll review the source before allowing access.").foregroundStyle(KeyalaStyle.muted) } }
            ForEach(store.grants) { grant in
                Button { selected = grant } label: { GrantCard(grant: grant) }.buttonStyle(.plain)
            }
            Text("Tap an app to change its access or disconnect it.").font(.caption).foregroundStyle(KeyalaStyle.muted)
        }.padding(20) }.background(KeyalaStyle.navy.ignoresSafeArea()).navigationBarTitleDisplayMode(.inline)
            .sheet(item: $selected) { item in GrantDetailView(grant: store.grants.first(where: { $0.id == item.id }) ?? item) }
    }
}

private struct CouponDetails: View {
    let coupon: Coupon
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("Sponsored credit", systemImage: "ticket").font(.headline).foregroundStyle(KeyalaStyle.teal)
            LabeledContent("Sponsor", value: coupon.app.name)
            LabeledContent("Remaining credit") {
                Text(coupon.remainingUSD, format: .currency(code: "USD").precision(.fractionLength(2...6)))
            }
            LabeledContent("Expires", value: coupon.expiresAt.asDate)
            LabeledContent("Status") { Text(LocalizedStringKey(coupon.status.title)) }
        }.font(.subheadline)
    }
}

private struct CouponsView: View {
    @EnvironmentObject private var store: AppStore
    @State private var coupons: [Coupon] = []
    @State private var error: String?
    @State private var loading = true

    var body: some View {
        List {
            if let error {
                Text(error).foregroundStyle(KeyalaStyle.warning)
                Button("Refresh") { Task { await load() } }
            } else if loading && coupons.isEmpty {
                ProgressView("Loading coupons…")
            } else if coupons.isEmpty {
                Text("No coupons claimed yet. Scan a coupon QR or enter its code to claim one.")
                    .foregroundStyle(KeyalaStyle.muted)
            }
            ForEach(coupons) { coupon in
                Section(coupon.app.name) {
                    CouponDetails(coupon: coupon)
                    LabeledContent("Provider", value: coupon.vendor.title)
                    LabeledContent("Eligible models", value: coupon.models.joined(separator: ", "))
                }
            }
        }.localizedNavigationTitle("Coupons")
            .task(id: store.contextGeneration) { coupons = []; await load() }
            .refreshable { await load() }
    }

    private func load() async {
        let generation = store.contextGeneration
        loading = true; error = nil
        defer { if generation == store.contextGeneration { loading = false } }
        do { coupons = try await store.coupons() }
        catch is CancellationError { }
        catch { if generation == store.contextGeneration && !Task.isCancelled { self.error = error.localizedDescription } }
    }
}

private struct CouponOfferDetails: View {
    let preview: CouponPreview
    var body: some View {
        LabeledContent("Sponsor", value: preview.app.name)
        LabeledContent("Provider", value: preview.vendor.title)
        LabeledContent("Eligible models", value: preview.models.joined(separator: ", "))
        LabeledContent("Coupon value") {
            Text(preview.perCouponUSD, format: .currency(code: "USD").precision(.fractionLength(2...6)))
        }
        LabeledContent("Claim by", value: preview.claimDeadline.asDate)
        LabeledContent("Valid after claim") { Text("\(preview.validityDays) days") }
    }
}

struct CouponClaimView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    let code: String
    @State private var preview: CouponPreview?
    @State private var claimed: Coupon?
    @State private var error: String?
    @State private var reload = 0
    @State private var claimTask: Task<Void, Never>?
    @State private var claiming = false

    var body: some View {
        Group {
            if store.account == nil {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Sign in to review this coupon").font(.headline)
                    Text("Your coupon link will stay here while you sign in.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                    AccountView()
                }.padding(.top)
            } else {
                Form {
                    Section {
                        LabeledContent("Keyala server", value: store.serverHost)
                        Text("The coupon covers eligible calls from its app. You review each call and approve it once. Connect to the app separately to let it request calls.")
                            .font(.footnote).foregroundStyle(KeyalaStyle.muted)
                    }
                    if let coupon = claimed ?? preview?.coupon {
                        Section {
                            Label("Coupon claimed", systemImage: "checkmark.circle.fill").foregroundStyle(KeyalaStyle.teal)
                            CouponDetails(coupon: coupon)
                            LabeledContent("Provider", value: coupon.vendor.title)
                            LabeledContent("Eligible models", value: coupon.models.joined(separator: ", "))
                        }
                    } else if let preview {
                        Section("Review coupon") {
                            CouponOfferDetails(preview: preview)
                            if preview.claimable {
                                Button("Claim coupon") { claim(preview) }.fontWeight(.semibold).disabled(claiming)
                            } else {
                                Text("This coupon is no longer available to claim.").foregroundStyle(KeyalaStyle.warning)
                            }
                        }
                    } else if error == nil {
                        ProgressView("Loading coupon…")
                    }
                    if claiming { ProgressView() }
                    if let error {
                        Section {
                            Text(error).foregroundStyle(KeyalaStyle.warning)
                            Button("Refresh") { reload += 1 }.disabled(claiming)
                        }
                    }
                }.scrollContentBackground(.hidden)
            }
        }.background(KeyalaStyle.navy).localizedNavigationTitle("Coupon")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") {
                        if store.pendingCouponCode == code { store.pendingCouponCode = nil }
                        dismiss()
                    }.disabled(claiming)
                }
            }
            .interactiveDismissDisabled(claiming)
            .task(id: "\(store.contextGeneration):\(store.linkGeneration):\(store.account?.id ?? ""):\(reload)") { await load() }
            .onDisappear { claimTask?.cancel() }
    }

    private func load() async {
        claimTask?.cancel(); claiming = false; preview = nil; claimed = nil; error = nil
        guard store.account != nil else { return }
        do { preview = try await store.couponPreview(code: code) }
        catch is CancellationError { }
        catch { if !Task.isCancelled { self.error = error.localizedDescription } }
    }

    private func claim(_ preview: CouponPreview) {
        guard !claiming else { return }
        claiming = true; error = nil
        claimTask = Task {
            defer { if !Task.isCancelled { claiming = false } }
            do { claimed = try await store.claimCoupon(code: code, preview: preview) }
            catch is CancellationError { }
            catch { if !Task.isCancelled { self.error = error.localizedDescription } }
        }
    }
}

struct GrantCard: View {
    let grant: GrantRecord
    var body: some View {
        Card { VStack(alignment: .leading, spacing: 14) {
            HStack { VStack(alignment: .leading, spacing: 4) { Text(grant.appName).font(.headline).foregroundStyle(KeyalaStyle.cream); Text(grant.model.isEmpty ? grant.provider.title : "\(grant.provider.title)  /  \(grant.model)").font(.caption).foregroundStyle(KeyalaStyle.muted) }; Spacer(); Image(systemName: "chevron.right").font(.caption.bold()).foregroundStyle(KeyalaStyle.muted) }
            if let coupon = grant.coupon {
                CouponDetails(coupon: coupon)
            } else {
                HStack(spacing: 14) {
                    metric("REQUESTS", value: grant.maxRequests.map { "\($0) left" } ?? "\(grant.usedRequests) used")
                    Rectangle().fill(.white.opacity(0.1)).frame(width: 1, height: 32)
                    metric("SPENT", value: grant.spentUSD)
                    Rectangle().fill(.white.opacity(0.1)).frame(width: 1, height: 32)
                    metric("POLICY", value: grant.trust ? "Trusted" : (grant.budgetMicrousd != nil ? "Budget" : (grant.maxRequests != nil ? "Count" : "Ask each time")))
                }
                if let remaining = grant.budgetMicrousd { Text(String(format: "$%.2f remaining", Double(remaining) / 1_000_000)).font(.caption).foregroundStyle(KeyalaStyle.teal) }
                Text(grant.expiresAt.isEmpty ? "No expiry" : "Expires \(grant.expiresAt.asDate)").font(.caption).foregroundStyle(KeyalaStyle.muted)
            }
        } }
    }
    func metric(_ name: String, value: String) -> some View { VStack(alignment: .leading, spacing: 4) { Text(name).font(.system(size: 9, weight: .bold)).tracking(0.8).foregroundStyle(KeyalaStyle.muted); Text(value).font(.subheadline.weight(.semibold)).foregroundStyle(KeyalaStyle.cream) }.frame(maxWidth: .infinity, alignment: .leading) }
}

struct GrantDetailView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    @State private var showPolicy = false
    @State private var confirmDisconnect = false
    @State private var updating = false
    let grant: GrantRecord
    private var current: GrantRecord { store.grants.first(where: { $0.id == grant.id }) ?? grant }
    var body: some View {
        NavigationStack {
            ScrollView { VStack(alignment: .leading, spacing: 18) {
                GrantCard(grant: current)
                if current.coupon == nil, current.billingVendor == .openRouter && current.provider != .openRouter {
                    Label("Billed through OpenRouter credits", systemImage: "arrow.triangle.branch").foregroundStyle(KeyalaStyle.teal)
                }
                InfoBanner()
                if current.coupon == nil {
                    Button { showPolicy = true } label: { Label("Change policy", systemImage: "slider.horizontal.3").frame(maxWidth: .infinity).padding(14).background(.white.opacity(0.08), in: Capsule()) }.disabled(updating)
                } else {
                    Text("Coupon calls must be approved one at a time.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                }
                Button(role: .destructive) { confirmDisconnect = true } label: { Label("Disconnect app", systemImage: "hand.raised.slash").frame(maxWidth: .infinity).padding(14).background(Color.red.opacity(0.16), in: Capsule()) }.disabled(updating)
                if updating { ProgressView() }
            }.padding(20) }.background(KeyalaStyle.navy.ignoresSafeArea()).localizedNavigationTitle(grant.appName).navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .topBarTrailing) { Button("Done") { dismiss() } } }
                .confirmationDialog("Disconnect this app?", isPresented: $confirmDisconnect, titleVisibility: .visible) {
                    Button("Disconnect app", role: .destructive) {
                        updating = true
                        Task { defer { updating = false }; if await store.revoke(grant) { dismiss() } }
                    }
                } message: {
                    Text(current.coupon == nil ? "This app loses permission to use this provider. Your stored key and past history stay. Requests already sent may still finish." : "This app loses access. Your coupon and past requests remain in Keyala.")
                }
                .sheet(isPresented: $showPolicy) {
                    ApprovalPolicyView(isApplying: updating) { policy in
                        guard !updating else { return }
                        updating = true
                        Task { defer { updating = false }; if await store.update(grant, policy: policy) { showPolicy = false } }
                    }
                }
                .onChange(of: store.contextGeneration) { _, _ in dismiss() }
        }
    }
}

struct AccountView: View {
    @EnvironmentObject private var store: AppStore
    @State private var stagingUsername = ""
    @State private var stagingPassword = ""
    @State private var confirmStop = false
    @State private var confirmSessions = false
    @State private var confirmSignOut = false

    var body: some View {
        Form {
            Section {
                PageHeader(eyebrow: "Keyala", title: "Account", subtitle: "Manage your sign-in.")
                InfoBanner()
            }.listRowBackground(KeyalaStyle.navy)
            Section("Profile") {
                if store.demoMode {
                    Label("Sample data", systemImage: "person.crop.circle")
                    Text("Turn off sample data in Developer options to sign in.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                } else if let account = store.account {
                    if let email = account.displayEmail {
                        LabeledContent("Email", value: email)
                    }
                    LabeledContent("Account ID", value: account.id)
                    LabeledContent("Server", value: store.serverURL)
                    if account.authProvider == "google" {
                        Text("Google manages your sign-in identity. This Keyala account is independent of Sign in with Apple.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                    }
                    if account.authProvider == "apple" {
                        Text("Apple manages your sign-in identity. Change your email or Apple Account settings with Apple. You can set a separate web password in Settings > Web password.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                    }
                    Button("Sign out on this iPhone") { confirmSignOut = true }.disabled(store.busy)
                } else {
                    AccountAuthenticationButtons(mode: .signIn)
                    Text("New here? Continuing creates your Keyala account. Returning users sign in to the same account.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                }
            }
            if !store.demoMode, store.account != nil {
                Section("Security & access") {
                    if store.authenticationMethods?.passkeys == true { NavigationLink("Passkeys") { PasskeysView() } }
                    Button("Sign out other devices") { confirmSessions = true }.disabled(store.busy)
                    Button("Stop all app permissions", role: .destructive) { confirmStop = true }.disabled(store.busy)
                    Text("Signing out devices does not revoke app permissions. Stop permissions separately if an app or token may be compromised.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                }
                Section("Delete account") {
                    NavigationLink("Delete Keyala account") { DeleteAccountView() }.disabled(store.busy)
                    Text("Permanently remove your Keyala account, keys, permissions and history. Your provider accounts and subscriptions are separate.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                }
            }
            if !store.demoMode, store.account == nil, store.supportsPasswordSignIn {
                Section("Password sign-in") {
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
            if store.developerOptions, !store.demoMode, store.account == nil, !store.supportsPasswordSignIn, store.serverURL != AppPreferences.productionURL {
                Section("Development sign-in") {
                    TextField("Developer email", text: $store.email)
                        .keyboardType(.emailAddress).textInputAutocapitalization(.never).autocorrectionDisabled()
                    Button("Sign in to development server") {
                        Task { store.saveSettings(); await store.signIn() }
                    }.disabled(store.busy)
                    Text("Available when the server enables development login.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                }
            }
        }.scrollContentBackground(.hidden).background(KeyalaStyle.navy.ignoresSafeArea()).navigationBarTitleDisplayMode(.inline)
        .task(id: store.serverURL) { await store.loadAuthenticationMethods() }
        .confirmationDialog("Sign out on this iPhone?", isPresented: $confirmSignOut, titleVisibility: .visible) {
            Button("Sign out", role: .destructive) { Task { await store.signOutRemotely() } }
        } message: { Text("This ends this device’s server session. Other devices and app permissions stay active.") }
        .confirmationDialog("Sign out other devices?", isPresented: $confirmSessions, titleVisibility: .visible) {
            Button("Sign out other devices", role: .destructive) { Task { await store.revokeOtherSessions() } }
        } message: { Text("This iPhone stays signed in. App permissions stay active until you stop them separately.") }
        .confirmationDialog("Stop all app permissions?", isPresented: $confirmStop, titleVisibility: .visible) {
            Button("Stop permissions", role: .destructive) { Task { await store.stopAllPermissions() } }
        } message: { Text("Every connected app will lose access. Unsent requests are denied. Already-sent requests may still finish; your keys stay stored.") }

    }
}

struct GoogleAuthenticationButton: View {
    @Environment(\.isEnabled) private var isEnabled
    var title: LocalizedStringKey = "Continue with Google"
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                Image("GoogleMark").renderingMode(.original).resizable().scaledToFit()
                    .frame(width: 22, height: 22)
                    .frame(width: 42, height: 42)
                    .background(.white, in: RoundedRectangle(cornerRadius: 3))
                    .accessibilityHidden(true)
                Text(title).font(.system(.body, design: .default, weight: .medium))
                    .foregroundStyle(.white).frame(maxWidth: .infinity)
                    .padding(.trailing, 42)
            }
            .padding(2).frame(maxWidth: .infinity, minHeight: 46)
            .background(Color(red: 66 / 255, green: 133 / 255, blue: 244 / 255), in: RoundedRectangle(cornerRadius: 5))
            .contentShape(RoundedRectangle(cornerRadius: 5))
        }.buttonStyle(.plain).opacity(isEnabled ? 1 : 0.55)
    }
}

struct AccountAuthenticationButtons: View {
    @EnvironmentObject private var store: AppStore
    let mode: SignInWithAppleButton.Label
    var body: some View {
        VStack(spacing: 12) {
            AppleAuthenticationButton(mode: mode)
            if store.authenticationMethods?.google == true {
                GoogleAuthenticationButton { Task { await store.signInGoogle() } }.disabled(store.busy)
                Text("Google creates a separate Keyala account. No Apple account is required.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
            }
            if store.authenticationMethods?.passkeys == true {
                Button { Task { await store.signInPasskey() } } label: { Label("Sign in with a passkey", systemImage: "person.badge.key") }
                    .frame(maxWidth: .infinity, minHeight: 46).buttonStyle(.bordered).disabled(store.busy)
            }
            if store.busy { ProgressView("Verifying account…") }
        }.task(id: store.serverURL) { await store.loadAuthenticationMethods() }
    }
}

struct PasskeysView: View {
    @EnvironmentObject private var store: AppStore
    @State private var removing: PasskeyRecord?
    @State private var adding = false
    var body: some View {
        Form {
            Section {
                Text("Passkeys sign in to this Keyala account with your device or credential manager. They do not require an Apple sign-in account.")
                Text("Create a passkey after signing in. To use another device, use your credential manager’s sync or the system’s nearby-device option.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                LabeledContent("Server", value: store.serverURL)
                Button("Add a passkey") { adding = true }.disabled(store.busy)
            }
            Section("Registered passkeys") {
                if store.passkeys.isEmpty { Text("No passkeys registered.").foregroundStyle(KeyalaStyle.muted) }
                ForEach(store.passkeys) { passkey in
                    VStack(alignment: .leading, spacing: 6) {
                        Text(passkey.name.isEmpty ? localized("Passkey") : passkey.name)
                        Text(passkey.createdAt.asDate).font(.caption).foregroundStyle(KeyalaStyle.muted)
                        Button("Remove passkey", role: .destructive) { removing = passkey }.disabled(store.busy)
                    }
                }
            }
            Section {
                Text("Removing a passkey disables it on this server. Your credential manager may keep its saved copy; remove that copy there if needed.").font(.footnote)
                InfoBanner()
            }
        }.scrollContentBackground(.hidden).background(KeyalaStyle.navy).localizedNavigationTitle("Passkeys")
            .sheet(isPresented: $adding) { NavigationStack { PasskeyEnrollmentVerificationView() } }
            .task(id: store.contextGeneration) { await store.loadPasskeys() }
            .confirmationDialog("Remove this passkey?", isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } }), titleVisibility: .visible) {
                if let passkey = removing { Button("Remove passkey", role: .destructive) { removing = nil; Task { await store.removePasskey(id: passkey.id) } } }
            } message: { Text("You will need another passkey or your original sign-in provider to sign in again.") }
    }
}

struct PasskeyEnrollmentVerificationView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    @Environment(\.colorScheme) private var colorScheme
    @State private var password = ""
    @State private var generation: Int?
    var body: some View {
        Form {
            Section {
                Text("Verify your identity before adding a passkey.")
                if store.account?.authProvider == "apple" {
                    SignInWithAppleButton(.continue, onRequest: { request in
                        generation = store.contextGeneration; request.requestedScopes = []
                    }, onCompletion: verifyApple)
                        .signInWithAppleButtonStyle(colorScheme == .dark ? .white : .black).frame(height: 46).disabled(store.busy)
                } else if store.account?.authProvider == "google", store.authenticationMethods?.google == true {
                    GoogleAuthenticationButton { Task { if await store.addPasskeyWithGoogle() { dismiss() } } }.disabled(store.busy)
                } else if store.account?.authProvider == "password" {
                    SecureField("Account password", text: $password).textContentType(.password)
                    Button("Verify and add passkey") {
                        let value = password; password = ""
                        Task { if await store.addPasskey(proof: FreshIdentity(password: value)) { dismiss() } }
                    }.disabled(store.busy || password.isEmpty)
                } else if store.account?.authProvider == "dev", store.developerOptions {
                    Button("Verify and add passkey") { Task { if await store.addPasskey(proof: FreshIdentity(email: store.account?.email)) { dismiss() } } }.disabled(store.busy)
                }
                if !store.passkeys.isEmpty {
                    Button("Verify with an existing passkey") { Task { if await store.addPasskeyWithExistingPasskey() { dismiss() } } }.disabled(store.busy)
                }
                if store.busy { ProgressView("Verifying account…") }
                InfoBanner()
            }
        }.localizedNavigationTitle("Add a passkey")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() }.disabled(store.busy) } }
            .onChange(of: store.contextGeneration) { _, _ in dismiss() }
            .interactiveDismissDisabled(store.busy)
    }
    private func verifyApple(_ result: Result<ASAuthorization, Error>) {
        do {
            let authorization = try result.get()
            guard generation == store.contextGeneration else { return }
            generation = nil
            guard let credential = authorization.credential as? ASAuthorizationAppleIDCredential,
                  let token = credential.identityToken.flatMap({ String(data: $0, encoding: .utf8) }) else { throw APIError.invalidResponse }
            Task { if await store.addPasskey(proof: FreshIdentity(idToken: token)) { dismiss() } }
        } catch {
            if (error as? ASAuthorizationError)?.code == .canceled { return }
            store.errorMessage = error.localizedDescription; store.showError = true
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
            PageHeader(eyebrow: "Keyala", title: "Your keys.\nYour permission.", subtitle: "Use AI apps with your own provider account. Review what each app sends before it runs.")
            VStack(alignment: .leading, spacing: 18) {
                Label("Add a provider key once", systemImage: "1.circle")
                Label("Scan an app’s connection QR", systemImage: "2.circle")
                Label("Review and approve requests", systemImage: "3.circle")
            }.font(.subheadline)
            AccountAuthenticationButtons(mode: .continue)
            Text("New here? Continuing creates your Keyala account. Returning users sign in to the same account.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
            Text("An experimental personal project by Joseph Chen, built with care. Provider usage is billed separately.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
            HStack { NavigationLink("Privacy & data") { PrivacyView() }; NavigationLink("Help & contact") { SupportView() } }.font(.footnote)
            if store.developerOptions { NavigationLink("Other sign-in options") { AccountView() } }
            InfoBanner()
        }.padding(24) }.background(KeyalaStyle.navy.ignoresSafeArea())
    }
}

struct SettingsView: View {
    @EnvironmentObject private var store: AppStore
    @AppStorage("allowSwipeOnce") private var allowSwipeOnce = false
    @AppStorage("requireDeviceAuthentication") private var requireDeviceAuthentication = false
    @AppStorage("hideOpenRouterRoutingGuide") private var hideRoutingGuide = false
    @State private var showSwipeWarning = false
    @State private var canAuthenticate = true
    var body: some View {
        Form {
            Section("Account") {
                NavigationLink { AccountView() } label: { Label(store.account == nil ? "Sign in" : "Account", systemImage: "person.crop.circle") }
                if store.account != nil && !store.demoMode {
                    NavigationLink("Web password") { WebPasswordView() }
                    NavigationLink("Sign in on the web") { WebSignInHelpView() }
                }
            }
            Section("Requests") {
                Button { Task { await store.requestNotifications() } } label: { Label("Notification permission", systemImage: "bell") }
                Toggle("Verify approvals on this device", isOn: $requireDeviceAuthentication).disabled(!canAuthenticate && !requireDeviceAuthentication)
                Text(canAuthenticate ? "Use Face ID, Touch ID or your device passcode before allowing a request." : "Set up a device passcode or biometrics in iPhone Settings to enable approval verification.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                Toggle("Swipe to allow", isOn: Binding(get: { allowSwipeOnce }, set: { value in
                    if value { showSwipeWarning = true } else { allowSwipeOnce = false }
                }))
                Text("Off by default. Only one-call approvals with an estimate of $0.05 or less can use this shortcut. Larger estimates open the review sheet.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
            }
            Section("OpenRouter credits") {
                Toggle("Use OpenRouter for OpenAI and Anthropic", isOn: Binding(
                    get: { store.routingPreferences.openRouterForDirectAPIs },
                    set: { enabled in Task { await store.setOpenRouterRouting(enabled) } }
                )).disabled(!store.routingPreferencesLoaded || store.updatingRoutingPreferences || (store.keys.allSatisfy { $0.provider != .openRouter } && !store.routingPreferences.openRouterForDirectAPIs))
                if store.updatingRoutingPreferences { ProgressView() }
                if store.account == nil { Text("Sign in to manage routing for this server.").font(.footnote) }
                else if !store.routingPreferencesLoaded { Text("Routing settings are unavailable. Check the server connection.").font(.footnote) }
                else if store.keys.allSatisfy({ $0.provider != .openRouter }) { Text("Connect OpenRouter in Keys first.").font(.footnote) }
                if !hideRoutingGuide {
                    Text("Off by default. New OpenAI and Anthropic app connections can use supported models with your OpenRouter key and credits. Requests pass through OpenRouter and its downstream providers; prices follow OpenRouter.").font(.footnote)
                    Text("Existing app connections keep their billing provider. Disconnect and reconnect an app to change its route. Unsupported models and API features are not converted.").font(.footnote)
                    Button("Don’t show this guide again") { hideRoutingGuide = true }
                } else { Button("Show routing guide") { hideRoutingGuide = false } }
            }.task(id: store.contextGeneration) { await store.loadRoutingPreferences() }
            Section("Language") { LanguagePicker() }
            Section("Appearance") { AppearancePicker() }
            Section("Help & privacy") {
                NavigationLink("Getting started") { FirstUseGuideView() }
                NavigationLink("Help & contact") { SupportView() }
                NavigationLink("Privacy & data") { PrivacyView() }
            }
            Section("Developer") { NavigationLink("Developer options") { DeveloperOptionsView() } }
            Section { InfoBanner() }
        }.scrollContentBackground(.hidden).background(KeyalaStyle.navy).localizedNavigationTitle("Settings")
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
                Text(store.demoMode || store.serverURL != AppPreferences.productionURL ? "Turn off sample data and select production before hiding developer options." : "For building and testing integrations. Normal use connects to Keyala automatically.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
            }
            if store.developerOptions {
                Section("Offline samples") {
                    Toggle("Use sample data", isOn: Binding(get: { store.demoMode }, set: { value in Task { await store.setDemo(value) } }))
                    Text("Fake requests and grants on this device only. No network requests, real keys, provider calls or charges. Switching this setting signs you out.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                    Text("To test a real integration without provider charges, run a private backend with KEYALA_VENDOR_MOCK=1 and fake keys. This switch cannot change a server’s mock setting.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                }
                Section("Server") {
                    Button("Use production") { store.useServer(AppPreferences.productionURL); serverDraft = store.serverURL }
                    Button("Use staging") { store.useServer(AppPreferences.stagingURL); serverDraft = store.serverURL }
                    Button("Use review server") { store.useServer(AppPreferences.reviewURL); serverDraft = store.serverURL }
                    TextField("Custom HTTPS URL", text: $serverDraft).keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
                    Button("Use custom server") { store.useServer(serverDraft) }.disabled(serverDraft == store.serverURL || store.demoMode)
                    Toggle("Allow HTTP for localhost", isOn: $store.allowLocalHTTP).onChange(of: store.allowLocalHTTP) { _, _ in store.saveSettings() }
                    Text("Changing servers signs you out. Remote servers require HTTPS; local HTTP is for a private development backend only.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                    LabeledContent("Current", value: store.serverURL).font(.caption)
                }.disabled(store.demoMode)
                if !store.demoMode { Section { NavigationLink("Development sign-in") { AccountView() } } }
            }
        }.scrollContentBackground(.hidden).background(KeyalaStyle.navy).localizedNavigationTitle("Developer options")
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
    private var couponCode: String? { CouponLink.code(from: code) }
    private var previewTaskID: String { "\(store.contextGeneration):\(store.account?.id ?? ""):\(code)" }

    var body: some View {
        Group {
            if store.account == nil && !store.demoMode {
                VStack(alignment: .leading, spacing: 16) {
                    Text("Sign in to review this connection").font(.headline)
                    Text("Your pairing link will stay here while you sign in.").foregroundStyle(KeyalaStyle.muted)
                    AccountView()
                }.padding(.top)
            } else {
                Form {
                    Section {
                        Button { Task { await openScanner() } } label: { Label("Scan QR code", systemImage: "qrcode.viewfinder").font(.headline).frame(maxWidth: .infinity).padding(8) }.disabled(deciding)
                        Text(addingKey ? "Scan the key setup QR on your computer. You’ll approve this computer before it can send a key." : "Scan the app’s connection QR or a coupon QR. You can also open its link on this phone.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                        if let scannerError { Text(scannerError).font(.footnote).foregroundStyle(KeyalaStyle.warning) }
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
                        TextField("KY-… or KC-…", text: $code).textInputAutocapitalization(.never).autocorrectionDisabled().disabled(deciding)
                        if loading { ProgressView("Loading connection…") }
                        else if couponCode != nil {
                            Button("Review coupon") { _ = store.acceptCoupon(code) }.disabled(deciding)
                        }
                        else if enteredCode != nil && !PairingLink.matchesServer(code, activeServer: store.serverURL) {
                            Text("This connection belongs to a different Keyala server. Choose the matching server in Settings > Developer options, then scan again. Your server has not changed.").font(.footnote).foregroundStyle(KeyalaStyle.warning)
                        }
                        else if !code.isEmpty && enteredCode == nil { Text("Enter the KY- connection code or KC- coupon code.").font(.footnote).foregroundStyle(KeyalaStyle.muted) }
                    } }
                    if let pairing, pairing.code == enteredCode, pairing.generation == store.contextGeneration {
                        let preview = pairing.preview
                        Section("Review connection") {
                            Text(preview.app.name).font(.title2.bold())
                            if let host = preview.sourceHost {
                                Label("From \(host)", systemImage: "globe").font(.headline).textSelection(.enabled)
                                Text("Check that this host matches the page you’re using.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                            } else {
                                Label("Unverified app: name chosen by the developer", systemImage: "exclamationmark.triangle").foregroundStyle(KeyalaStyle.warning)
                            }
                            if let coupon = preview.coupon {
                                if let claimed = coupon.coupon { CouponDetails(coupon: claimed) }
                                else { CouponOfferDetails(preview: coupon) }
                                Text("This connection uses sponsored credit for eligible calls. Your own keys stay unused. Every call needs your approval.")
                                    .font(.footnote).foregroundStyle(KeyalaStyle.muted)
                                if coupon.canConnect {
                                    Button(coupon.coupon == nil ? "Claim coupon and connect" : "Connect with coupon") { decide(approve: true) }.disabled(deciding)
                                } else {
                                    Text("This coupon is unavailable for this connection.").foregroundStyle(KeyalaStyle.warning)
                                }
                            } else {
                                if preview.keyProvider == preview.vendor { Text("Requests access to your \(preview.vendor.title) key.") }
                                else { Text("Requests the \(preview.vendor.title) API through OpenRouter.") }
                                if preview.keyProvider == .openRouter && preview.vendor != .openRouter {
                                    Label("Billed through OpenRouter credits", systemImage: "arrow.triangle.branch").foregroundStyle(KeyalaStyle.teal)
                                    Text("This connection uses supported models through OpenRouter, including its downstream providers, instead of a direct provider key.").font(.footnote)
                                }
                                let hasKey = store.keys.contains(where: { $0.provider == preview.keyProvider })
                                if !hasKey {
                                    NavigationLink("Add your \(preview.keyProvider.title) key first") { VaultView(returnConnectCode: pairing.code, suggestedProvider: preview.keyProvider) }
                                }
                                Text("Each call needs your approval at first. Change access later in Connected apps.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                                Button(preview.requiresAsk == false ? "Choose access" : "Connect and ask each time") {
                                    if preview.requiresAsk == false { showPairPolicy = true } else { decide(approve: true) }
                                }.disabled(deciding || !hasKey)
                            }
                            Button("Deny connection", role: .destructive) { decide(approve: false) }.disabled(deciding)
                        }
                    }
                }.scrollContentBackground(.hidden)
            }
        }.background(KeyalaStyle.navy).localizedNavigationTitle(addingKey ? "Scan key setup QR" : "Connect an app").navigationBarTitleDisplayMode(.inline)
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
                    .ignoresSafeArea(edges: .bottom).localizedNavigationTitle(addingKey ? "Scan key setup QR" : "Scan Keyala QR")
                    .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { showScanner = false } } }
                }
            }
    }
    private func loadPreview() async {
        pairing = nil
        loading = false
        guard let value = enteredCode, store.account != nil || store.demoMode else { return }
        guard PairingLink.matchesServer(code, activeServer: store.serverURL) else { return }
        let generation = store.contextGeneration
        loading = true
        defer { if generation == store.contextGeneration { loading = false } }
        guard let preview = await store.connectPreview(code: value), !Task.isCancelled,
              generation == store.contextGeneration, enteredCode == value else { return }
        pairing = (value, generation, preview)
    }
    private func decide(approve: Bool, policy: Policy = .once) {
        guard !deciding, let pairing, pairing.code == enteredCode, pairing.generation == store.contextGeneration else { return }
        deciding = true
        Task {
            defer { deciding = false }
            if await store.connectDecision(code: pairing.code, approve: approve, policy: policy, preview: pairing.preview),
               enteredCode == pairing.code, pairing.generation == store.contextGeneration {
                if store.pendingConnectCode == pairing.code { store.pendingConnectCode = nil }
                dismiss()
            } else if approve, pairing.generation == store.contextGeneration {
                await loadPreview()
            }
        }
    }
    private func processScannedValue() {
        guard let value = scannedValue, scannerGeneration == store.contextGeneration, scannerLinkGeneration == store.linkGeneration else { return }
        scannedValue = nil
        code = ""; pairing = nil
        if CouponLink.code(from: value) != nil { _ = store.acceptCoupon(value) }
        else if WebLoginLink.id(from: value) != nil { _ = store.acceptWebLogin(value) }
        else if EnrollmentLink.id(from: value) != nil { _ = store.acceptEnrollment(value) }
        else if PairingLink.code(from: value) != nil {
            if store.acceptPairing(value), let scanned = store.pendingConnectCode { code = scanned }
        } else { scannerError = "Scan a Keyala connection, coupon, web sign-in or key setup QR." }
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
                Text("Choose a provider. The page will show a QR code; no Keyala password is needed on the computer.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
            }
            Section("On this phone") {
                NavigationLink { ConnectAppView(addingKey: true) } label: { Label("Scan setup QR", systemImage: "qrcode.viewfinder").font(.headline) }
                Text("Check the server and provider, then approve the computer. Paste the key on the computer and confirm its last four characters here before saving.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
            }
            Section {
                Text("Only a short-lived setup identifier is in the QR. Your key stays out of QR codes and phone enrollment screens.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
            }
        }.scrollContentBackground(.hidden).background(KeyalaStyle.navy).localizedNavigationTitle("Add from computer").navigationBarTitleDisplayMode(.inline)
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
                    Text("The setup link will stay here while you sign in.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                    AccountView()
                }.padding(.top)
            } else {
                Form {
                    Section("Key setup server") {
                        Label(store.serverHost, systemImage: "lock.shield").font(.headline).textSelection(.enabled)
                        Text("Check that this matches the address open on your computer. Approve only a setup you started yourself.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                    }
                    if let errorMessage {
                        Section {
                            Text(errorMessage).foregroundStyle(KeyalaStyle.warning)
                            Button("Check setup status") { reload += 1 }.disabled(busy)
                        }
                    }
                    if loading && enrollment == nil { Section { ProgressView("Loading key setup…") } }
                    if saved || enrollment?.status == "saved" {
                        Section {
                            Label("Key saved", systemImage: "checkmark.circle.fill").font(.headline).foregroundStyle(KeyalaStyle.teal)
                            Text("Your apps can use this provider with your permission. They never receive the key.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
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
                                Text("The key will only be saved after you review and confirm it here.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                                Button("Approve this computer") { claim(enrollment) }.fontWeight(.semibold).disabled(busy || errorMessage != nil)
                            } else if enrollment.status == "claimed" {
                                ProgressView("Waiting for your computer…")
                                Text("Create or copy your key in the provider’s console, then paste it on the computer’s Keyala page. Keep this screen open.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                                Button("Cancel key setup", role: .destructive) { cancel(enrollment) }.disabled(busy)
                            } else if enrollment.canConfirm {
                                Text("Review the submitted key").font(.headline)
                                LabeledContent("Provider", value: enrollment.vendor.title)
                                LabeledContent("New key ending", value: enrollment.last4 ?? "")
                                if let current = enrollment.currentLast4 {
                                    Label("This replaces your saved \(enrollment.vendor.title) key ending \(current).", systemImage: "exclamationmark.triangle").foregroundStyle(KeyalaStyle.warning)
                                }
                                Text("Check that these last four characters match the key you pasted. The full key is never sent to this phone.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
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
        }.background(KeyalaStyle.navy).localizedNavigationTitle("Add provider key").navigationBarTitleDisplayMode(.inline)
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
        let plain = formatter.date(from: self)
        formatter.formatOptions.insert(.withFractionalSeconds)
        guard let date = plain ?? formatter.date(from: self) else { return self }
        let output = DateFormatter(); output.locale = AppLanguage.selected.locale; output.dateStyle = .medium; output.timeStyle = .short
        return output.string(from: date)
    }
}


struct SupportView: View {
    var body: some View {
        Form {
            Section("Contact") {
                Link(destination: SupportContact.mailURL) { Label("Email support", systemImage: "envelope") }
                Text(SupportContact.email).textSelection(.enabled)
                Text("Include your app version and the steps that failed. Never send API keys, authorization headers, passwords or private prompts.").font(.footnote).foregroundStyle(KeyalaStyle.muted)
                LabeledContent("App version", value: (Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0") + " (" + (Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "1") + ")")
            }
            Section("Provider credits") {
                Text("Keyala beta is free. Model usage is billed separately to your provider account. A ChatGPT or Claude subscription does not supply API credits. OpenRouter uses its own balance.")
                Text("If a provider reports insufficient credits, check the billing account or project that issued your key. Adding a key does not add credits.")
            }
            Section("Lost phone or leaked access") {
                Text("On another iPhone, sign in to the same Keyala account. In Settings > Account, sign out other devices and stop all app permissions. Revoke a leaked provider key at the provider too; removing it from Keyala does not revoke it there.")
                ForEach(Provider.allCases) { provider in Link(provider.title, destination: provider.keysURL) }
            }
            Section("A request is uncertain") {
                Text("The provider may already have processed the request. Check its recorded status and your provider usage before starting another request. Keyala does not automatically retry an uncertain model call.")
            }
            Section("Connecting and notifications") {
                Text("Scan the connection QR on the app’s website and check its source before allowing access. If a code expires, start a new connection on that website.")
                Text("If notifications are disabled, open Inbox to review waiting requests. Notification permission can be changed in iPhone Settings.")
            }
        }.scrollContentBackground(.hidden).background(KeyalaStyle.navy).localizedNavigationTitle("Help & contact").navigationBarTitleDisplayMode(.inline)
    }
}

struct PrivacyView: View {
    var body: some View {
        Form {
            Section("Who operates Keyala") {
                Text("Keyala is an experimental personal project operated by Joseph Chen. I am actively developing and testing it, but cannot promise uninterrupted service or immediate support. Contact support@keyala.io with questions.")
            }
            Section("Your keys") {
                Text("Provider keys are sent to your selected Keyala server and encrypted there. Apps receive scoped permission tokens, never your original key. The server decrypts a key to forward an allowed request; its operator must still be trusted.")
            }
            Section("Requests and history") {
                Text("Keyala stores request contents, provider responses, timestamps and usage-derived costs so you can inspect them later. Approved request contents are sent to the selected provider, and the calling app receives the response. That app may keep its own copy.")
                Text("When OpenRouter routing is enabled, requests go through OpenRouter and its downstream providers and use OpenRouter credits.")
                Text("Treat prompts and responses as account data. Avoid including information you do not want the selected provider or calling app to receive.")
            }
            Section("Control and support") {
                Text("Remove a provider key in Keys or disconnect an app in Apps. Removing a key does not delete your provider account or cancel a provider subscription. Already-sent requests cannot be recalled.")
                Text("Delete your Keyala account in Settings > Account. You must confirm and verify your identity again. Stored keys, permissions and request history are removed from the live service. Older backups may retain data until they expire; deletion records must be replayed before a restored database is used.")
                Text("The beta does not add advertising trackers or third-party analytics SDKs. Apple and hosting providers may process service and diagnostic data under their own policies.")
                Link("Privacy questions", destination: SupportContact.mailURL)
            }
        }.scrollContentBackground(.hidden).background(KeyalaStyle.navy).localizedNavigationTitle("Privacy & data").navigationBarTitleDisplayMode(.inline)
    }
}

struct FirstUseGuideView: View {
    @Environment(\.dismiss) private var dismiss
    var onFinish: (() -> Void)? = nil
    @State private var step = 0
    private let titles = ["Your keys, your decision", "Choose a provider account", "Connect, review, stay in control"]
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack(spacing: 16) {
                    Image("BrandMark").resizable().scaledToFit().frame(width: 44, height: 44).accessibilityHidden(true)
                    LanguagePicker()
                }
                Text("Getting started").font(.subheadline).foregroundStyle(KeyalaStyle.teal)
                HStack { ForEach(0..<3) { index in Capsule().fill(index == step ? KeyalaStyle.teal : KeyalaStyle.line).frame(height: 4) } }.accessibilityHidden(true)
                Text(LocalizedStringKey(titles[step])).font(.title.bold()).fixedSize(horizontal: false, vertical: true)
                if step == 0 {
                    Text("An experimental personal project by Joseph Chen, built with care. The beta is free; provider usage is billed separately.")
                    Text("Your selected Keyala server stores encrypted keys and decrypts them to forward approved requests. You must trust its operator. Apps get permission tokens, not your original key.")
                    NavigationLink("Read privacy & data") { PrivacyView() }
                } else if step == 1 {
                    Text("After signing in, open Keys. Connect OpenRouter with its authorization screen, or add an OpenAI or Anthropic API key from your phone or computer.")
                    Text("OpenRouter has its own credits. ChatGPT and Claude subscriptions do not include OpenAI or Anthropic API credits. Adding a key does not add a balance.")
                    Text("Start with a separate provider key and a small provider-side spending limit where available.")
                } else {
                    Text("Start connecting from a supported app’s website. Scan its QR, check the source, then review the actual request and estimated cost before allowing one call.")
                    Text("Notifications are optional. Open Inbox to check waiting requests, Activity to inspect results, and Apps to revoke access. Settings > Account has account deletion and security controls.")
                    Text("If a request is uncertain, check the original request and provider usage before starting another. It may already have run.")
                }
                HStack {
                    if step > 0 { Button("Back") { step -= 1 }.buttonStyle(.bordered) }
                    Spacer()
                    Button {
                        if step < 2 { step += 1 } else { if let onFinish { onFinish() } else { dismiss() } }
                    } label: { Text(LocalizedStringKey(step == 2 ? "Get started" : "Next")) }.buttonStyle(.borderedProminent)
                }.padding(.top, 12)
            }.padding(20)
        }.id(step).background(KeyalaStyle.navy.ignoresSafeArea()).foregroundStyle(KeyalaStyle.cream)
            .navigationBarTitleDisplayMode(.inline)
    }
}

struct DeleteAccountView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    @Environment(\.colorScheme) private var colorScheme
    @State private var confirmation = ""
    @State private var password = ""
    @State private var appleReauthGeneration: Int?
    var body: some View {
        Form {
            Section("What will be deleted") {
                Text("Your Keyala account, stored provider keys, app permissions, sessions, and request and response history will be permanently removed from this server.")
                Text("Provider accounts, subscriptions and data already received by providers or apps are separate. Backups can retain older copies until expiration; deletion records must be applied before a restore is served.")
                Text("If a provider request is being sent, wait for it to finish and try deletion again. Deletion cannot recall a request already sent.")
            }
            Section("Confirm account deletion") {
                TextField("Type DELETE to confirm", text: $confirmation).textInputAutocapitalization(.characters).autocorrectionDisabled()
                if store.account?.authProvider == "apple" {
                    Text("Continue with Apple to verify the same account again. Completing verification deletes your Keyala account.").font(.footnote)
                    SignInWithAppleButton(.continue, onRequest: { request in
                        appleReauthGeneration = store.contextGeneration
                        request.requestedScopes = []
                    }, onCompletion: deleteWithApple)
                        .signInWithAppleButtonStyle(colorScheme == .dark ? .white : .black).frame(height: 46)
                        .disabled(confirmation != "DELETE" || store.busy)
                } else if store.account?.authProvider == "google" {
                    Text("Verify the same Google account again to permanently delete this Keyala account.").font(.footnote)
                    GoogleAuthenticationButton(title: "Verify with Google and delete") { Task { await store.deleteWithGoogle() } }
                        .disabled(confirmation != "DELETE" || store.busy || store.authenticationMethods?.google != true)
                } else if store.account?.authProvider == "password" {
                    SecureField("Account password", text: $password).textContentType(.password)
                    Button("Permanently delete account", role: .destructive) {
                        let value = password; password = ""
                        Task { await store.deleteAccount(AccountDeletionRequest(password: value)) }
                    }.disabled(confirmation != "DELETE" || password.isEmpty || store.busy)
                } else if store.account?.authProvider == "dev", store.developerOptions {
                    Button("Permanently delete account", role: .destructive) {
                        let email = store.account?.email
                        Task { await store.deleteAccount(AccountDeletionRequest(email: email)) }
                    }.disabled(confirmation != "DELETE" || store.busy)
                } else {
                    Text("Sign in again to confirm your account type before deleting it. If this server has not been updated, contact support.")
                    Link("Contact support", destination: SupportContact.mailURL)
                }
                if !store.passkeys.isEmpty, store.authenticationMethods?.passkeys == true {
                    Button("Verify with passkey and delete", role: .destructive) { Task { await store.deleteWithPasskey() } }
                        .disabled(confirmation != "DELETE" || store.busy)
                }
                if store.busy { ProgressView("Verifying account…") }
            }
        }.scrollContentBackground(.hidden).background(KeyalaStyle.navy).localizedNavigationTitle("Delete account").navigationBarTitleDisplayMode(.inline)
            .task(id: store.contextGeneration) {
                await store.loadAuthenticationMethods()
                if store.authenticationMethods?.passkeys == true { await store.loadPasskeys() }
            }
            .onChange(of: store.account?.id) { _, id in if id == nil { dismiss() } }
    }
    private func deleteWithApple(_ result: Result<ASAuthorization, Error>) {
        do {
            let authorization = try result.get()
            guard appleReauthGeneration == store.contextGeneration else { return }
            appleReauthGeneration = nil
            guard confirmation == "DELETE", let credential = authorization.credential as? ASAuthorizationAppleIDCredential,
                  let token = credential.identityToken.flatMap({ String(data: $0, encoding: .utf8) }) else { throw APIError.invalidResponse }
            let code = credential.authorizationCode.flatMap { String(data: $0, encoding: .utf8) }
            Task { await store.deleteAccount(AccountDeletionRequest(idToken: token, authorizationCode: code)) }
        } catch {
            if (error as? ASAuthorizationError)?.code == .canceled { return }
            store.errorMessage = error.localizedDescription; store.showError = true
        }
    }
}


struct WebLoginApprovalView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    let id: String
    @State private var preview: WebLoginPreview?
    @State private var error: String?
    @State private var deciding = false
    @State private var action: Task<Void, Never>?
    @State private var now = Date()
    private let timer = Timer.publish(every: 1, on: .main, in: .common).autoconnect()
    var body: some View {
        Group {
            if store.account == nil { AccountView() }
            else {
                Form {
                    Section {
                        Label("Sign in on the web", systemImage: "desktopcomputer").font(.title2.bold())
                        Text("Only approve if you just opened Keyala in your own browser and scanned its sign-in QR. A QR sent by someone else can sign them into your account.")
                        LabeledContent("Keyala server", value: store.serverHost)
                    }
                    if let preview {
                        Section("Review browser") {
                            LabeledContent("Browser", value: preview.browser ?? "Unknown")
                            LabeledContent("Country", value: preview.country ?? "Unknown")
                            LabeledContent("Created", value: preview.createdAt)
                            LabeledContent("Expires", value: preview.expiresAt)
                            Text("Browser details are hints, not verified identity.").font(.footnote)
                            Text("The web console can view your keys’ last four characters, connected apps and request history, and manage your developer apps. Key changes and spending approvals stay on your phone.").font(.footnote)
                            if preview.isPending(at: now) {
                                Button("Approve web sign-in") { decide(true) }.disabled(deciding)
                                Button("Deny web sign-in", role: .destructive) { decide(false) }.disabled(deciding)
                            } else { Text("This sign-in is no longer pending. Start again in your browser.") }
                        }
                    } else if error == nil { ProgressView("Loading sign-in…") }
                    if deciding { ProgressView() }
                    if let error { Section { Text(error).foregroundStyle(KeyalaStyle.warning); Button("Refresh") { preview = nil; self.error = nil; load() }.disabled(deciding) } }
                }.scrollContentBackground(.hidden)
            }
        }.background(KeyalaStyle.navy).localizedNavigationTitle("Web sign-in")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() }.disabled(deciding) } }
            .task(id: "\(store.contextGeneration):\(store.account?.id ?? "")") { preview = nil; error = nil; load() }
            .onReceive(timer) { now = $0 }
            .onDisappear { action?.cancel() }
    }
    private func load() {
        action?.cancel()
        action = Task {
            do { let value = try await store.webLoginPreview(id: id); guard !Task.isCancelled else { return }; preview = value }
            catch { if !Task.isCancelled { self.error = error.localizedDescription } }
        }
    }
    private func decide(_ approve: Bool) {
        deciding = true; error = nil
        action = Task {
            defer { deciding = false }
            do { try await store.decideWebLogin(id: id, approve: approve); guard !Task.isCancelled else { return }; dismiss() }
            catch { if !Task.isCancelled { self.error = error.localizedDescription } }
        }
    }
}

struct WebPasswordView: View {
    @EnvironmentObject private var store: AppStore
    @State private var password = ""
    @State private var confirmation = ""
    @State private var busy = false
    @State private var remove = false
    @State private var message: String?
    @State private var action: Task<Void, Never>?
    var body: some View {
        Form {
            Section {
                Text("Sign in to the web console with your account email and a separate password. Your Apple Account password stays with Apple.")
                LabeledContent("Username", value: store.account?.email ?? "")
                Text("Use at least 12 characters. Scan the web sign-in QR instead if you prefer not to set a password.").font(.footnote)
            }
            Section("Web password") {
                SecureField("New web password", text: $password).textContentType(.newPassword)
                SecureField("Confirm web password", text: $confirmation).textContentType(.newPassword)
                Button("Save web password") { update(password) }.disabled(busy || !(12...200).contains(password.unicodeScalars.count) || password != confirmation)
                Button("Remove web password", role: .destructive) { remove = true }.disabled(busy)
                if busy { ProgressView() }
                if let message { Text(message).font(.footnote) }
            }.disabled(store.account == nil || store.demoMode)
        }.scrollContentBackground(.hidden).background(KeyalaStyle.navy).localizedNavigationTitle("Web password")
            .confirmationDialog("Remove web password?", isPresented: $remove) {
                Button("Remove web password", role: .destructive) { update(nil) }
            } message: { Text("You can still sign in by scanning the web sign-in QR with Keyala.") }
            .onChange(of: store.contextGeneration) { _, _ in action?.cancel(); password = ""; confirmation = ""; message = nil }
            .onDisappear { action?.cancel(); password = ""; confirmation = "" }
    }
    private func update(_ value: String?) {
        busy = true; message = nil; password = ""; confirmation = ""
        action = Task {
            defer { busy = false }
            do { try await store.setWebPassword(value); guard !Task.isCancelled else { return }; message = localized(value == nil ? "Web password removed." : "Web password saved.") }
            catch { if !Task.isCancelled { message = error.localizedDescription } }
        }
    }
}

struct WebSignInHelpView: View {
    @EnvironmentObject private var store: AppStore
    @State private var link = ""
    var body: some View {
        Form {
            Section("On your computer") {
                if let url = URL(string: store.serverURL)?.appendingPathComponent("console") {
                    Text(url.absoluteString).textSelection(.enabled)
                    ShareLink(item: url) { Text("Send console link") }
                }
                Text("Choose phone sign-in. Scan the QR using your iPhone Camera or Keyala’s QR scanner, then review and approve the browser.")
            }
            Section("Can’t scan? Paste the sign-in link") {
                TextField("https://app.keyala.io/login/…", text: $link).textInputAutocapitalization(.never).autocorrectionDisabled().keyboardType(.URL)
                Button("Review web sign-in") { _ = store.acceptWebLogin(link) }.disabled(WebLoginLink.id(from: link) == nil)
            }
        }.scrollContentBackground(.hidden).background(KeyalaStyle.navy).localizedNavigationTitle("Web sign-in")
    }
}
