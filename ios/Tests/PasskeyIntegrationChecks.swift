import Foundation
import CryptoKit

// Disposable localhost only. Exercises native JSON against the real Rust verifier;
// the system authentication sheet and associated domains require a signed device.
@main struct PasskeyIntegrationChecks {
    static func main() async throws {
        let address = CommandLine.arguments.dropFirst().first ?? "http://127.0.0.1:8099"
        guard let host = URL(string: address)?.host, ["127.0.0.1", "localhost"].contains(host) else { fatalError("localhost only") }
        let api = APIClient(baseURL: address, allowLocalHTTP: true, token: nil)
        let email = "passkey-\(UUID().uuidString.lowercased())@example.test"
        let session = try await api.devLogin(email: email)
        api.token = session.sessionToken
        do { _ = try await api.passkeyRegistrationOptions(proof: FreshIdentity()); fatalError("Session alone must not enroll a passkey") }
        catch APIError.reauthenticationFailed { }
        let options = try await api.passkeyRegistrationOptions(proof: FreshIdentity(email: email))
        let key = P256.Signing.PrivateKey()
        let point = Array(key.publicKey.x963Representation)
        let credentialID = Data(SHA256.hash(data: Data(UUID().uuidString.utf8)))
        let rpHash = Data(SHA256.hash(data: Data(options.rpID.utf8)))
        let cose = Data([0xa5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21, 0x58, 0x20] + Array(point[1...32]) + [0x22, 0x58, 0x20] + Array(point[33...64]))
        let registrationData = rpHash + Data([0x45, 0, 0, 0, 0]) + Data(repeating: 0, count: 16) + Data([0, UInt8(credentialID.count)]) + credentialID + cose
        // CBOR map: fmt=none, attStmt={}, authData=<bytes>.
        let attestationParts = [Data([0xa3, 0x63]), Data("fmt".utf8), Data([0x64]), Data("none".utf8), Data([0x67]), Data("attStmt".utf8), Data([0xa0, 0x68]), Data("authData".utf8), Data([0x58, UInt8(registrationData.count)]), registrationData]
        let attestation = attestationParts.reduce(into: Data()) { $0.append($1) }

        func clientData(_ type: String, _ challenge: String) throws -> Data {
            try JSONSerialization.data(withJSONObject: ["type": type, "challenge": challenge, "origin": "https://" + options.rpID])
        }
        let registration = PasskeyCredential(id: credentialID.base64URL, response: .init(clientDataJSON: try clientData("webauthn.create", options.challenge).base64URL, attestationObject: attestation.base64URL))
        let record = try await api.registerPasskey(PasskeyProof(challengeID: options.challengeID, credential: registration))
        let records = try await api.passkeys()
        precondition(records.contains { $0.id == record.id })
        try await api.removePasskey(id: record.id)
        let removed = try await api.passkeys()
        precondition(removed.isEmpty)
        let replacementOptions = try await api.passkeyRegistrationOptions(proof: FreshIdentity(email: email))
        let replacement = PasskeyCredential(id: credentialID.base64URL, response: .init(clientDataJSON: try clientData("webauthn.create", replacementOptions.challenge).base64URL, attestationObject: attestation.base64URL))
        _ = try await api.registerPasskey(PasskeyProof(challengeID: replacementOptions.challengeID, credential: replacement))
        func assertion(_ challenge: PasskeyRequestOptions, count: UInt8) throws -> PasskeyProof {
            let cd = try clientData("webauthn.get", challenge.challenge)
            let authData = rpHash + Data([0x05, 0, 0, 0, count])
            let signature = try key.signature(for: authData + Data(SHA256.hash(data: cd))).derRepresentation
            return PasskeyProof(challengeID: challenge.challengeID, credential: PasskeyCredential(id: credentialID.base64URL, response: .init(clientDataJSON: cd.base64URL, authenticatorData: authData.base64URL, signature: signature.base64URL, userHandle: options.userID)))
        }
        let challenge = try await api.passkeyOptions()
        let proof = try assertion(challenge, count: 1)
        let login = try await api.passkeyLogin(proof)
        precondition(login.user.id == session.user.id && login.user.authProvider == "dev")
        do { _ = try await api.passkeyLogin(proof); fatalError("Passkey challenge must be single use") } catch APIError.http(400, _) { }
        // The same typed assertion body must work as fresh deletion identity.
        let deletionChallenge = try await api.passkeyOptions()
        let result = try await api.deleteAccount(AccountDeletionRequest(passkey: assertion(deletionChallenge, count: 2)))
        precondition(result.deleted)
        print("Native Passkey API checks passed: fresh enrollment, real P-256 verification, list/removal, login identity, replay rejection, fresh deletion")
    }
}
