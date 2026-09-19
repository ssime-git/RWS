import CryptoKit
import Foundation

// Validate the pair using Apple's implementation; never print key material.
let env = ProcessInfo.processInfo.environment
guard let encoded = env["RWS_SPARKLE_PRIVATE_KEY"],
      let seed = Data(base64Encoded: encoded.trimmingCharacters(in: .whitespacesAndNewlines)),
      seed.count == 32,
      let publicText = env["RWS_SPARKLE_PUBLIC_KEY"],
      let publicBytes = Data(base64Encoded: publicText),
      let key = try? Curve25519.Signing.PrivateKey(rawRepresentation: seed),
      key.publicKey.rawRepresentation == publicBytes else {
    fputs("Sparkle key pair mismatch; export a new-format 32-byte seed with generate_keys.\n", stderr)
    exit(1)
}
print("Sparkle key pair verified.")
