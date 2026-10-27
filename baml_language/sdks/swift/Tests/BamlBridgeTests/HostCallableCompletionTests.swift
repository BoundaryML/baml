import Foundation
import XCTest

@testable import BamlBridge

/// How a host callable's call ends: which outcome each fault sends (ABI.md,
/// "A callback has one completion channel"). Each case runs one invocation
/// of a `BamlHostCallable` on a hand-built `BamlToHostCall`, as the engine
/// would dispatch it, and reads the outcome it would complete with.
final class HostCallableCompletionTests: XCTestCase {
    /// `(int) -> string`, open throws.
    private let intToString = BamlTypeDescriptor.function(
        params: [BamlFunctionParamDescriptor(type: .int)],
        returns: .string,
        throws: .unknown
    )

    private func call(
        positional: [BamlBridge_Cffi_V1_BamlOutboundValue],
        named: [(String, BamlBridge_Cffi_V1_BamlOutboundValue)] = []
    ) throws -> Data {
        var call = BamlBridge_Cffi_V1_BamlToHostCall()
        call.args = positional.map { value in
            var arg = BamlBridge_Cffi_V1_BamlToHostArg()
            arg.value = value
            return arg
        } + named.map { name, value in
            var arg = BamlBridge_Cffi_V1_BamlToHostArg()
            arg.value = value
            arg.argName = name
            arg.isOptionalArg = true
            return arg
        }
        return try call.serializedData()
    }

    private func int(_ value: Int64) -> BamlBridge_Cffi_V1_BamlOutboundValue {
        var raw = BamlBridge_Cffi_V1_BamlOutboundValue()
        raw.intValue = value
        return raw
    }

    private func string(_ value: String) -> BamlBridge_Cffi_V1_BamlOutboundValue {
        var raw = BamlBridge_Cffi_V1_BamlOutboundValue()
        raw.stringValue = value
        return raw
    }

    /// `(int) -> string` whose body is `body`, given its decoded argument.
    private func callable(
        thrown: [BamlThrownSlot] = [],
        _ body: @escaping @Sendable (Int) async throws -> String
    ) -> BamlHostCallable {
        BamlHostCallable(functionType: intToString, thrown: thrown) { args in
            try args.result(try await body(try args.required(0, as: BamlSlot<Int>.own)), as: .own)
        }
    }

    private func assertBridgeFailure(
        _ outcome: BamlBridge_Cffi_V1_HostCallCompletion.OneOf_Outcome,
        _ fragment: String,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        guard case .bridgeFailure(let message) = outcome else {
            return XCTFail("expected bridge_failure, got \(outcome)", file: file, line: line)
        }
        XCTAssertTrue(message.contains(fragment), message, file: file, line: line)
    }

    func testAValueThatFitsIsReturned() async throws {
        let outcome = await callable { "got \($0)" }.invoke(try call(positional: [int(5)]))
        guard case .returned(let value) = outcome else {
            return XCTFail("expected returned, got \(outcome)")
        }
        XCTAssertEqual(value.stringValue, "got 5")
    }

    /// Arguments that are not the ones the callable declares are the
    /// bridge's fault, never the callable's: one positional argument too
    /// many or too few, or an optional one it does not have.
    func testArgumentsOutsideTheSignatureAreABridgeFailure() async throws {
        let echo = callable { "\($0)" }
        assertBridgeFailure(
            await echo.invoke(try call(positional: [int(1), int(2)])),
            "takes 1 positional argument(s) but was passed 2"
        )
        assertBridgeFailure(await echo.invoke(try call(positional: [])), "but was passed 0")
        assertBridgeFailure(
            await echo.invoke(try call(positional: [int(1)], named: [("extra", int(2))])),
            "`extra`, which is not one of its optional parameters"
        )
    }

    func testAnArgumentTheBridgeCannotDecodeIsABridgeFailure() async throws {
        let ran = Flag()
        let outcome = await callable { _ in
            ran.set()
            return ""
        }.invoke(try call(positional: [string("not an int")]))
        assertBridgeFailure(outcome, "positional argument 0 could not be decoded")
        XCTAssertFalse(ran.isSet, "the body ran on arguments the bridge could not decode")
    }

    func testUnreadableArgumentsAreABridgeFailure() async {
        assertBridgeFailure(await callable { "\($0)" }.invoke(Data([0xFF])), "could not be read")
    }

    func testACallCancelledBeforeItRunsIsABridgeFailure() async throws {
        let data = try call(positional: [int(1)])
        let echo = callable { "\($0)" }
        let task = Task {
            withUnsafeCurrentTask { $0?.cancel() }
            return await echo.invoke(data)
        }
        assertBridgeFailure(await task.value, "cancelled before it ran")
    }

    /// A handle the bridge cannot clone for the result is its own failure,
    /// not a trap.
    func testAHandleThatCannotBeClonedIsABridgeFailure() async throws {
        let stale = BamlHandle(key: 0xDEAD_0001, handleType: .untaggedBexHeap)
        let callable = BamlHostCallable(
            functionType: .function(
                params: [BamlFunctionParamDescriptor(type: .int)],
                returns: .rustType,
                throws: .unknown
            ),
            thrown: []
        ) { args in
            _ = try args.required(0, as: BamlSlot<Int>.own)
            return try args.result(stale, as: ._leaf(.rustType))
        }
        assertBridgeFailure(await callable.invoke(try call(positional: [int(1)])), "handle_clone failed")
    }

    /// A result nested deeper than the wire carries below the completion is
    /// the bridge's failure; one level shallower crosses. A list of `Nest`
    /// costs two messages a level and its innermost statement two more, so 48
    /// levels nest 99 messages under the completion and 49 nest 101.
    func testAResultTooDeepForTheWireIsABridgeFailure() async throws {
        func returning(_ depth: Int) -> BamlHostCallable {
            BamlHostCallable(
                functionType: .function(
                    params: [BamlFunctionParamDescriptor(type: .int)],
                    returns: Nest._bamlStatedType,
                    throws: .unknown
                ),
                thrown: []
            ) { args in
                _ = try args.required(0, as: BamlSlot<Int>.own)
                return try args.result(Nest.nested(depth), as: .own)
            }
        }
        guard case .returned = await returning(48).invoke(try call(positional: [int(1)])) else {
            return XCTFail("48 levels fit under the completion")
        }
        assertBridgeFailure(
            await returning(49).invoke(try call(positional: [int(1)])),
            "101 wire messages deep"
        )
        // Past the encoder's own bound too: refused before it descends.
        assertBridgeFailure(
            await returning(60).invoke(try call(positional: [int(1)])),
            "more than 50 deep"
        )
    }

    /// A thrown value is encoded against the declared throws type, not its
    /// own Swift type: a `Box<String>` thrown under `throws Box<"a">` states
    /// `Box<"a">`, which it inhabits; `Box<string>` would not fit (classes
    /// are invariant).
    func testAThrownValueIsEncodedAgainstTheDeclaredThrowsType() async throws {
        let declared = BamlSlot<Box<String>>._classType("user.Box", BamlSlot<String>.literal("a"))
        let outcome = await callable(thrown: [BamlThrownSlot(declared)]) { _ in
            throw BamlThrownValue(Box(value: "a"))
        }.invoke(try call(positional: [int(1)]))
        guard case .thrown(let value) = outcome, case .classTy(let cls)? = value.valueType.ty else {
            return XCTFail("expected a thrown class, got \(outcome)")
        }
        XCTAssertEqual(cls.name, "user.Box")
        XCTAssertEqual(cls.typeArgs, [BamlTypeDescriptor.literal("a").raw])
    }

    /// Under an open throws type the value states its own type.
    func testAThrownValueUnderAnOpenThrowsTypeStatesItsOwnType() async throws {
        let outcome = await callable { _ in throw BamlThrownValue(Box(value: "a")) }
            .invoke(try call(positional: [int(1)]))
        guard case .thrown(let value) = outcome, case .classTy(let cls)? = value.valueType.ty else {
            return XCTFail("expected a thrown class, got \(outcome)")
        }
        XCTAssertEqual(cls.typeArgs, [BamlTypeDescriptor.string.raw])
    }

    /// A `BamlError` or `BamlPanic` from a nested BAML call goes back as the
    /// value it carried, restated: a panic stays a panic whatever the
    /// callable declares, which the engine decides from the value.
    func testAForwardedBamlErrorOrPanicIsThrownAsItsValue() async throws {
        var field = BamlBridge_Cffi_V1_BamlOutboundMapEntry()
        field.key = "message"
        field.value = string("boom")
        var cls = BamlBridge_Cffi_V1_BamlValueClass()
        cls.name = "baml.panics.UserPanic"
        cls.fields = [field]
        var payload = BamlBridge_Cffi_V1_BamlOutboundValue()
        payload.classValue = cls

        for forwarded: any Error in [
            BamlPanic(message: "boom", className: cls.name, payload: BamlOutboundValue(payload)),
            BamlError(message: "boom", className: cls.name, payload: BamlOutboundValue(payload)),
        ] {
            let outcome = await callable { _ in throw forwarded }.invoke(try call(positional: [int(1)]))
            guard case .thrown(let value) = outcome, case .classTy(let type)? = value.valueType.ty else {
                return XCTFail("expected a thrown class, got \(outcome)")
            }
            XCTAssertEqual(type.name, "baml.panics.UserPanic")
            XCTAssertEqual(value.classValue.fields.first?.stringKey, "message")
            XCTAssertEqual(value.classValue.fields.first?.value.stringValue, "boom")
        }
    }

    /// Restating keeps every container's exact type and drops the union
    /// selection around a value.
    func testAForwardedValueIsRestatedWithTheTypesTheEngineGaveIt() throws {
        var items = BamlBridge_Cffi_V1_BamlValueList()
        items.itemType = BamlTypeDescriptor.literal("a").raw
        items.items = [string("a")]
        var list = BamlBridge_Cffi_V1_BamlOutboundValue()
        list.listValue = items
        var variant = BamlBridge_Cffi_V1_BamlValueUnionVariant()
        variant.selfType = BamlTypeDescriptor.union([.list(.literal("a")), .int]).raw
        variant.selectedOptionIndex = 0
        variant.value = list
        var wrapped = BamlBridge_Cffi_V1_BamlOutboundValue()
        wrapped.unionVariantValue = variant

        let restated = try restate(wrapped)
        XCTAssertEqual(restated.valueType, BamlTypeDescriptor.list(.literal("a")).raw)
        XCTAssertEqual(restated.listValue.values.map(\.stringValue), ["a"])

        var runtime = BamlBridge_Cffi_V1_BamlTyClass()
        runtime.head = .live(7)
        var unstatable = BamlBridge_Cffi_V1_BamlValueList()
        unstatable.itemType.classTy = runtime
        var refused = BamlBridge_Cffi_V1_BamlOutboundValue()
        refused.listValue = unstatable
        XCTAssertThrowsError(try restate(refused))
    }

    /// A wrapper built host-side carries no BAML value: it is the callable's
    /// own exception, like any other Swift error.
    func testAnErrorWithoutABamlValueIsRaised() async throws {
        struct Mine: Error {}
        for raised: any Error in [Mine(), BamlError(message: "host-built")] {
            let outcome = await callable { _ in throw raised }.invoke(try call(positional: [int(1)]))
            guard case .raised(let exception) = outcome else {
                return XCTFail("expected raised, got \(outcome)")
            }
            XCTAssertEqual(exception.language, "swift")
            HostCallableRegistry.shared.release(exception.handle)
        }
    }

    /// Swift spells a cancelled BAML call `CancellationError`; one let escape
    /// a callable goes back as the `baml.panics.Cancelled` it stands for.
    func testACancellationErrorIsThrownAsTheCancelledPanic() async throws {
        let outcome = await callable { _ in throw CancellationError() }.invoke(try call(positional: [int(1)]))
        guard case .thrown(let value) = outcome, case .classTy(let type)? = value.valueType.ty else {
            return XCTFail("expected a thrown class, got \(outcome)")
        }
        XCTAssertEqual(type.name, "baml.panics.Cancelled")
    }

    /// Engine handles and host values are numbered independently: equal
    /// numbers in different key spaces are different handles.
    func testHandleIdentityIncludesItsKeySpace() {
        let host = BamlHandle(key: 0xDEAD_0002, handleType: .hostValueOpaque)
        let engine = BamlHandle(key: 0xDEAD_0002, handleType: .untaggedBexHeap)
        let sameEngineRow = BamlHandle(key: 0xDEAD_0002, handleType: .adtTaggedHeapHandle)
        XCTAssertNotEqual(host, engine)
        XCTAssertEqual(engine, sameEngineRow)
    }
}

/// `"a"[] | "b"[]` and `Box<"a"> | Box<"b">` are two cases of one Swift type
/// each; only the declared arm the engine selected says which case a value
/// is, so a union decodes through its declared slot.
final class LiteralUnionDecodeTests: XCTestCase {
    private func selecting(
        _ index: UInt32,
        of options: [BamlTypeDescriptor],
        _ value: BamlBridge_Cffi_V1_BamlOutboundValue
    ) -> BamlOutboundValue {
        var variant = BamlBridge_Cffi_V1_BamlValueUnionVariant()
        variant.selfType = BamlTypeDescriptor.union(options).raw
        variant.selectedOptionIndex = index
        variant.value = value
        var wrapped = BamlBridge_Cffi_V1_BamlOutboundValue()
        wrapped.unionVariantValue = variant
        return BamlOutboundValue(wrapped)
    }

    func testAUnionOfLiteralListsDecodesTheSelectedArm() throws {
        let slot = BamlSlots.union(
            BamlSlots.list(BamlSlot<String>.literal("a")),
            BamlSlots.list(BamlSlot<String>.literal("b"))
        )
        var item = BamlBridge_Cffi_V1_BamlOutboundValue()
        item.stringValue = "b"
        var list = BamlBridge_Cffi_V1_BamlValueList()
        list.itemType = BamlTypeDescriptor.literal("b").raw
        list.items = [item]
        var raw = BamlBridge_Cffi_V1_BamlOutboundValue()
        raw.listValue = list

        let decoded = try BamlUnion2<[String], [String]>._bamlDecode(
            selecting(1, of: [.list(.literal("a")), .list(.literal("b"))], raw),
            as: slot
        )
        XCTAssertEqual(decoded, .t1(["b"]))
        // Decoded, it re-encodes under the arm it came from.
        XCTAssertEqual(try decoded._bamlEncode(as: slot).raw.valueType, BamlTypeDescriptor.list(.literal("b")).raw)
    }

    func testAUnionOfLiteralArgumentClassesDecodesTheSelectedArm() throws {
        let slot = BamlSlots.union(
            BamlSlot<Box<String>>._classType("user.Box", BamlSlot<String>.literal("a")),
            BamlSlot<Box<String>>._classType("user.Box", BamlSlot<String>.literal("b"))
        )
        var field = BamlBridge_Cffi_V1_BamlOutboundMapEntry()
        field.key = "value"
        field.value.stringValue = "a"
        var cls = BamlBridge_Cffi_V1_BamlValueClass()
        cls.name = "user.Box"
        cls.typeArgs = [BamlTypeDescriptor.literal("a").raw]
        cls.fields = [field]
        var raw = BamlBridge_Cffi_V1_BamlOutboundValue()
        raw.classValue = cls

        let box = { (argument: String) in BamlTypeDescriptor.classType("user.Box", typeArguments: [.literal(argument)]) }
        let decoded = try BamlUnion2<Box<String>, Box<String>>._bamlDecode(
            selecting(0, of: [box("a"), box("b")], raw),
            as: slot
        )
        XCTAssertEqual(decoded, .t0(Box(value: "a")))
    }

    /// A merged leaf case (`"a" | "b"`, one `String` case) is selected by any
    /// of its members.
    func testAMergedLeafCaseIsSelectedByEachOfItsMembers() throws {
        let slot = BamlSlots.union(
            BamlSlot<String>.oneOf(.literal("a"), .literal("b")),
            BamlSlot<Int>.own
        )
        var raw = BamlBridge_Cffi_V1_BamlOutboundValue()
        raw.stringValue = "b"
        let decoded = try BamlUnion2<String, Int>._bamlDecode(
            selecting(1, of: [.literal("a"), .literal("b"), .int], raw),
            as: slot
        )
        XCTAssertEqual(decoded, .t0("b"))
    }
}

/// Lists, maps and classes nest at most `BamlValueNesting.maxDepth` deep in
/// the encoder; whether a value within that crosses is the wire's exact
/// measure, taken once it is built.
final class ArgumentWireDepthTests: XCTestCase {
    /// A `Nest` list costs two messages a level and its innermost statement
    /// two more; an argument sits two messages below the request. So 48
    /// levels nest 100 messages and cross, and 49 nest 102 and are refused as
    /// the engine refuses an argument it cannot read.
    func testAnArgumentIsMeasuredExactly() throws {
        _ = try encodeCallArgs(
            [BamlArgument("value", Nest.nested(48), as: .own)],
            callId: 7,
            callTarget: .functionName("f")
        )
        XCTAssertThrowsError(try encodeCallArgs(
            [BamlArgument("value", Nest.nested(49), as: .own)],
            callId: 7,
            callTarget: .functionName("f")
        )) { error in
            guard let error = error as? BamlError else {
                return XCTFail("expected BamlError, got \(error)")
            }
            XCTAssertEqual(error.className, "baml.errors.InvalidArgument")
            XCTAssertTrue(error.message.contains("102 wire messages deep"), error.message)
        }
    }

    /// The measure is the engine's: a statement's own nesting counts, and a
    /// scalar costs nothing.
    func testTheMeasureCountsStatementsAndPayloads() throws {
        XCTAssertEqual(BamlWireDepth.inbound(try 1._bamlEncode().raw), 0)
        // `BamlTy` → `BamlTyList` → `BamlTy` → `BamlTyPrimitive` = 4, deeper
        // than the empty list's 1.
        XCTAssertEqual(BamlWireDepth.inbound(try [Int]()._bamlEncode().raw), 4)
        // `InboundMapValue` → `InboundMapEntry` → `InboundValue` (3) above a
        // stated `int[]` (4), deeper than the `map<string, int[]>`
        // statement (6).
        XCTAssertEqual(BamlWireDepth.inbound(try ["k": [1]]._bamlEncode().raw), 7)
    }
}

private final class Flag: @unchecked Sendable {
    private let lock = NSLock()
    private var value = false

    func set() {
        lock.lock()
        defer { lock.unlock() }
        value = true
    }

    var isSet: Bool {
        lock.lock()
        defer { lock.unlock() }
        return value
    }
}

/// Hand-written stand-in for a generated generic class.
private struct Box<T: BamlCodableValue>: BamlTyped, BamlDecodable, Equatable, Sendable {
    var value: T

    static var _bamlSlot: BamlSlot<Self> { ._classType("user.Box", BamlSlot<T>.own) }

    func _bamlEncode(as slot: BamlSlot<Self>) throws -> BamlInboundValue {
        let argument0: BamlSlot<T> = slot._classArgument(0)
        return try .baml_class(slot, [BamlField("value", self.value, as: argument0)])
    }

    static func _bamlDecode(_ v: BamlOutboundValue) throws -> Self {
        try _bamlDecode(v, as: _bamlSlot)
    }

    static func _bamlDecode(_ v: BamlOutboundValue, as slot: BamlSlot<Self>) throws -> Self {
        let argument0: BamlSlot<T> = slot._classArgument(0)
        return Box(value: try v.classFields()._baml("value", as: argument0))
    }
}

/// A recursive alias of lists, shaped as sdkgen_swift emits one.
private indirect enum Nest: BamlTyped, BamlLeafEncodable {
    case leaf
    case list([Nest])

    static var _bamlSlot: BamlSlot<Self> { ._leaf(.typeAlias("user.Nest")) }

    /// Lists nested `depth` deep around a leaf.
    static func nested(_ depth: Int) -> Nest {
        (0..<depth).reduce(Nest.leaf) { inner, _ in .list([inner]) }
    }

    func _bamlEncode(as _: BamlSlot<Self>) throws -> BamlInboundValue {
        switch self {
        case .leaf: return 0._bamlEncode(as: BamlSlot<Int>.own)
        case .list(let items): return try items._bamlEncode(as: BamlSlots.list(BamlSlot<Nest>.own))
        }
    }
}
