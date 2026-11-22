import Foundation
import AVFoundation

// Built into the macOS client; a separate process isolates codec/plugin failures.
let args = CommandLine.arguments
func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data(message.utf8))
    exit(1)
}
guard args.count == 4, args[1] == "proxy" else { fail("Expected proxy source output") }
let asset = AVURLAsset(url: URL(fileURLWithPath: args[2]))
let destination = URL(fileURLWithPath: args[3])
guard let session = AVAssetExportSession(asset: asset, presetName: AVAssetExportPreset1280x720) else {
    fail("This Mac cannot export this video with the requested preset")
}
session.outputFileType = .mp4
session.outputURL = destination
session.shouldOptimizeForNetworkUse = true
try? FileManager.default.removeItem(at: destination)
let done = DispatchSemaphore(value: 0)
session.exportAsynchronously { done.signal() }
done.wait()
guard session.status == .completed else {
    fail(session.error?.localizedDescription ?? "Native video export failed")
}
