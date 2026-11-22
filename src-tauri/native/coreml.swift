import Foundation
import CoreML
import Metal

private func failure(_ message: String) -> NSError {
    NSError(domain: "ClipStudioCoreML", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
}
private final class Encoder {
    let vision: MLModel
    private let configuration: MLModelConfiguration
    private var models: [Int: MLModel] = [:]
    private var order: [Int] = []
    init(_ visionPath: String, _ textPath: String) throws {
        guard MTLCreateSystemDefaultDevice() != nil else { throw failure("No Metal GPU is available") }
        configuration = MLModelConfiguration()
        configuration.computeUnits = .cpuAndGPU
        configuration.allowLowPrecisionAccumulationOnGPU = false
        vision = try MLModel(contentsOf: URL(fileURLWithPath: visionPath), configuration: configuration)
        _ = try text(textPath, 32)
    }
    func text(_ path: String, _ length: Int) throws -> MLModel {
        if let model = models[length] {
            order.removeAll { $0 == length }
            order.append(length)
            return model
        }
        // Retain the query bucket and the current asset bucket without retaining every context.
        if order.count >= 2 { models.removeValue(forKey: order.removeFirst()) }
        let model = try MLModel(contentsOf: URL(fileURLWithPath: path), configuration: configuration)
        guard let shape = model.modelDescription.inputDescriptionsByName["embedded_tokens"]?.multiArrayConstraint?.shape,
              shape.map({ $0.intValue }) == [1, length, 512]
        else { throw failure("Unexpected native backbone shape") }
        models[length] = model
        order.append(length)
        return model
    }
}
private func report(_ error: Error, _ buffer: UnsafeMutablePointer<CChar>, _ capacity: Int) {
    let bytes = Array(error.localizedDescription.utf8.prefix(max(0, capacity - 1)))
    for (index, byte) in bytes.enumerated() { buffer[index] = CChar(bitPattern: byte) }
    buffer[bytes.count] = 0
}
private func array(_ source: UnsafeRawPointer, _ shape: [Int], _ type: MLMultiArrayDataType) throws -> MLMultiArray {
    let value = try MLMultiArray(shape: shape.map(NSNumber.init), dataType: type)
    memcpy(value.dataPointer, source, value.count * 4)
    return value
}
private func predict(_ model: MLModel, _ inputs: [String: Any], _ output: UnsafeMutablePointer<Float>, _ count: Int) throws {
    let result = try model.prediction(from: MLDictionaryFeatureProvider(dictionary: inputs))
    guard let name = result.featureNames.first, let tensor = result.featureValue(for: name)?.multiArrayValue,
          tensor.count == count else { throw failure("Unexpected native output shape") }
    var contiguous = true
    var stride = 1
    for axis in (0..<tensor.shape.count).reversed() {
        if tensor.shape[axis].intValue > 1 && tensor.strides[axis].intValue != stride { contiguous = false }
        stride *= tensor.shape[axis].intValue
    }
    if contiguous && tensor.dataType == .float32 { memcpy(output, tensor.dataPointer, count * 4) }
    else { for index in 0..<count { output[index] = tensor[index].floatValue } }
}
@_cdecl("cs_coreml_compile")
public func compileModel(_ source: UnsafePointer<CChar>, _ destination: UnsafePointer<CChar>, _ error: UnsafeMutablePointer<CChar>, _ capacity: Int) -> Int32 {
    autoreleasepool {
        do {
            let compiled = try MLModel.compileModel(at: URL(fileURLWithPath: String(cString: source)))
            defer { try? FileManager.default.removeItem(at: compiled) }
            try FileManager.default.copyItem(at: compiled, to: URL(fileURLWithPath: String(cString: destination)))
            return 0
        } catch let failure { report(failure, error, capacity); return -1 }
    }
}
@_cdecl("cs_coreml_open")
public func openEncoder(_ vision: UnsafePointer<CChar>, _ text: UnsafePointer<CChar>, _ error: UnsafeMutablePointer<CChar>, _ capacity: Int) -> UnsafeMutableRawPointer? {
    autoreleasepool {
        do { return Unmanaged.passRetained(try Encoder(String(cString: vision), String(cString: text))).toOpaque() }
        catch let failure { report(failure, error, capacity); return nil }
    }
}
@_cdecl("cs_coreml_close")
public func closeEncoder(_ handle: UnsafeMutableRawPointer) { Unmanaged<Encoder>.fromOpaque(handle).release() }
@_cdecl("cs_coreml_vision")
public func vision(_ handle: UnsafeMutableRawPointer, _ pixels: UnsafePointer<Float>, _ positions: UnsafePointer<Int32>, _ output: UnsafeMutablePointer<Float>, _ error: UnsafeMutablePointer<CChar>, _ capacity: Int) -> Int32 {
    autoreleasepool {
        do {
            let model = Unmanaged<Encoder>.fromOpaque(handle).takeUnretainedValue()
            try predict(model.vision, ["pixel_values": array(pixels, [1, 2520, 768], .float32), "pixel_position_ids": array(positions, [1, 2520, 2], .int32)], output, 280 * 512)
            return 0
        } catch let failure { report(failure, error, capacity); return -1 }
    }
}
@_cdecl("cs_coreml_body")
public func body(_ handle: UnsafeMutableRawPointer, _ path: UnsafePointer<CChar>, _ embedded: UnsafePointer<Float>, _ ids: UnsafePointer<Int32>, _ mask: UnsafePointer<Int32>, _ positions: UnsafePointer<Float>, _ length: Int, _ output: UnsafeMutablePointer<Float>, _ error: UnsafeMutablePointer<CChar>, _ capacity: Int) -> Int32 {
    autoreleasepool {
        do {
            let encoder = Unmanaged<Encoder>.fromOpaque(handle).takeUnretainedValue()
            let model = try encoder.text(String(cString: path), length)
            try predict(model, ["embedded_tokens": array(embedded, [1, length, 512], .float32), "input_ids": array(ids, [1, length], .int32), "attention_mask": array(mask, [1, length], .int32), "positions": array(positions, [length], .float32)], output, 768)
            return 0
        } catch let failure { report(failure, error, capacity); return -1 }
    }
}
