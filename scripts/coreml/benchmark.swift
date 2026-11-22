import Foundation
import CoreML

struct TensorInput: Decodable {
    let name: String
    let path: String
    let shape: [Int]
    let type: String
}
struct PredictionCase: Decodable {
    let name: String
    let output: String
    let inputs: [TensorInput]
}
enum BenchmarkError: Error {
    case invalidArguments, invalidTensor, invalidOutput
}
func emit(_ record: [String: Any]) throws {
    let data = try JSONSerialization.data(withJSONObject: record, options: [.sortedKeys])
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data([10]))
}
func array(_ input: TensorInput) throws -> MLMultiArray {
    guard !input.shape.isEmpty, input.shape.allSatisfy({ $0 > 0 }),
          ["float32", "int32"].contains(input.type) else { throw BenchmarkError.invalidTensor }
    var count = 1
    for dimension in input.shape {
        let (product, overflow) = count.multipliedReportingOverflow(by: dimension)
        guard !overflow && product <= Int.max / 4 else { throw BenchmarkError.invalidTensor }
        count = product
    }
    let bytes = try Data(contentsOf: URL(fileURLWithPath: input.path), options: [.mappedIfSafe])
    guard bytes.count == count * 4 else { throw BenchmarkError.invalidTensor }
    let value = try MLMultiArray(shape: input.shape.map(NSNumber.init),
                                 dataType: input.type == "int32" ? .int32 : .float32)
    _ = bytes.withUnsafeBytes { memcpy(value.dataPointer, $0.baseAddress!, bytes.count) }
    return value
}
func save(_ tensor: MLMultiArray, _ destination: String) throws {
    var values = [Float](repeating: 0, count: tensor.count)
    var contiguous = true
    var stride = 1
    for axis in (0..<tensor.shape.count).reversed() {
        if tensor.shape[axis].intValue > 1 && tensor.strides[axis].intValue != stride {
            contiguous = false
        }
        stride *= tensor.shape[axis].intValue
    }
    if contiguous && tensor.dataType == .float32 {
        values.withUnsafeMutableBytes { target in
            _ = memcpy(target.baseAddress!, tensor.dataPointer, tensor.count * 4)
        }
    } else {
        for index in values.indices { values[index] = tensor[index].floatValue }
    }
    guard values.allSatisfy({ $0.isFinite }) else { throw BenchmarkError.invalidOutput }
    try values.withUnsafeBytes { try Data($0).write(to: URL(fileURLWithPath: destination), options: [.atomic]) }
}
func main() throws {
    guard CommandLine.arguments.count == 3 else { throw BenchmarkError.invalidArguments }
    let cases = try JSONDecoder().decode([PredictionCase].self,
        from: Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[2])))
    let configuration = MLModelConfiguration()
    configuration.computeUnits = .cpuAndGPU
    try emit(["stage": "loading", "computePolicy": "cpuAndGPU"])
    let start = DispatchTime.now().uptimeNanoseconds
    let model = try MLModel(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]),
                            configuration: configuration)
    try emit(["stage": "loaded", "milliseconds": Double(DispatchTime.now().uptimeNanoseconds - start) / 1e6])
    for sample in cases {
        try autoreleasepool {
            var inputs: [String: Any] = [:]
            for tensor in sample.inputs {
                guard inputs[tensor.name] == nil else { throw BenchmarkError.invalidTensor }
                inputs[tensor.name] = try array(tensor)
            }
            let provider = try MLDictionaryFeatureProvider(dictionary: inputs)
            try emit(["stage": "predicting", "case": sample.name])
            let start = DispatchTime.now().uptimeNanoseconds
            let output = try model.prediction(from: provider)
            let milliseconds = Double(DispatchTime.now().uptimeNanoseconds - start) / 1e6
            guard output.featureNames.count == 1,
                  let name = output.featureNames.first,
                  let tensor = output.featureValue(for: name)?.multiArrayValue
            else { throw BenchmarkError.invalidOutput }
            try save(tensor, sample.output)
            try emit(["stage": "predicted", "case": sample.name, "milliseconds": milliseconds,
                      "shape": tensor.shape.map { $0.intValue }, "count": tensor.count])
        }
    }
    try emit(["stage": "done"])
}
do { try main() }
catch {
    try? emit(["stage": "failed", "error": String(describing: error)])
    exit(1)
}
