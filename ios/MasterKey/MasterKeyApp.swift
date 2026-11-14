import SwiftUI

@main
struct MasterKeyApp: App {
    @Environment(\.scenePhase) private var scenePhase
    @UIApplicationDelegateAdaptor(APNSDelegate.self) private var appDelegate
    @StateObject private var store = AppStore()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(store)
                .preferredColorScheme(.dark)
                .tint(MKStyle.teal)
                .task { await store.start() }
                .task {
                    while !Task.isCancelled {
                        try? await Task.sleep(for: .seconds(15))
                        guard !Task.isCancelled else { return }
                        if scenePhase == .active { await store.refresh() }
                    }
                }
                .onReceive(NotificationCenter.default.publisher(for: UIApplication.willEnterForegroundNotification)) { _ in
                    Task { await store.refresh() }
                }
                .onReceive(NotificationCenter.default.publisher(for: .masterKeyAPNSToken)) { note in
                    if let token = note.object as? String { Task { await store.uploadDeviceToken(token) } }
                }
                .onReceive(NotificationCenter.default.publisher(for: .masterKeyRequestTapped)) { note in
                    if let id = note.object as? String {
                        store.pendingRequestID = id
                        Task { await store.refresh() }
                    }
                }
                .onReceive(NotificationCenter.default.publisher(for: .masterKeyAPNSError)) { note in
                    if let message = note.object as? String { store.infoMessage = "Push registration unavailable: \(message). Foreground refresh still works." }
                }
        }
    }
}

enum MKStyle {
    static let navy = Color(red: 0.035, green: 0.075, blue: 0.12)
    static let panel = Color(red: 0.075, green: 0.13, blue: 0.18)
    static let teal = Color(red: 0.25, green: 0.82, blue: 0.72)
    static let cream = Color(red: 0.94, green: 0.92, blue: 0.84)
    static let muted = Color(red: 0.59, green: 0.68, blue: 0.69)
}

struct RootView: View {
    @EnvironmentObject private var store: AppStore
    @State private var selection = 0

    var body: some View {
        TabView(selection: $selection) {
            NavigationStack { InboxView() }
                .tabItem { Label("Requests", systemImage: "checkmark.shield") }.tag(0)
            NavigationStack { VaultView() }
                .tabItem { Label("Keys", systemImage: "key.horizontal") }.tag(1)
            NavigationStack { GrantsView() }
                .tabItem { Label("Grants", systemImage: "hand.raised") }.tag(2)
            NavigationStack { SettingsView() }
                .tabItem { Label("Settings", systemImage: "slider.horizontal.3") }.tag(3)
        }
        .background(MKStyle.navy)
        .toolbarBackground(MKStyle.navy, for: .tabBar)
        .toolbarBackground(.visible, for: .tabBar)
        .alert("MasterKey", isPresented: $store.showError) {
            Button("OK", role: .cancel) { }
        } message: { Text(store.errorMessage) }
        .onOpenURL { store.handleOpenURL($0); if store.pendingConnectCode != nil { selection = 3 } }
        .onChange(of: store.pendingRequestID) { _, id in if id != nil { selection = 0 } }
    }
}
