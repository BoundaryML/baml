/** Owned base for generated interface callers. Native handles are the authority;
 * diagnostic names and JavaScript class shapes cannot grant implementation. */
import { newFunctionCall } from './native.js';
import { baml_bridge } from './proto/baml_cffi.js';
import { attachCallContext } from './call_context.js';
import { decodeCallResult, encodeCallArgs } from './proto.js';
import { BamlType, _typeConstruction, _composeTypeDefinition, isBamlType } from './wire_ty.js';
/** A generated interface type with fixed ordinary and associated arguments. */
export class BamlInterfaceType extends BamlType {
    constructor(reference, name, definition, children) {
        super(_typeConstruction, definition, typeMap => {
            if (typeMap.getInterface(name) !== reference) {
                throw new TypeError("target Ref type does not belong to this reference's SDK");
            }
            for (const child of children)
                child._checkSdk(typeMap);
        });
        Object.freeze(this);
    }
    /** @internal Generated declarations supply the exact argument layout. */
    static _create(reference, name, args, bindings) {
        const children = [...args, ...bindings.map(([, ty]) => ty)];
        const definition = _composeTypeDefinition(children, types => ({ interface: {
                name, typeArgs: types.slice(0, args.length),
                bindings: bindings.map(([name], index) => ({ name, ty: types[args.length + index] })),
            } }));
        return new BamlInterfaceType(reference, name, definition, children);
    }
}
export class BamlRef {
    #handle;
    #typeMap;
    constructor(handle, typeMap) {
        this.#handle = handle;
        this.#typeMap = typeMap;
    }
    /** Decoder-only factory. Adoption belongs to the enclosing result receipt. */
    static _fromHandle(handle, typeMap) {
        const reference = new this(handle, typeMap);
        Object.freeze(reference);
        return reference;
    }
    /** Encoder-only borrowing; the encoder clones a separate wire lease. */
    _toHandle() { return this.#handle; }
    close() { this.#handle.close(); }
    toJSON() {
        throw new TypeError('A live BAML reference cannot be serialized; export its application data explicitly');
    }
    clone() {
        const ctor = this.constructor;
        return ctor._fromHandle(this.#handle.clone(), this.#typeMap);
    }
    /** Check an exact view on the same receiver; never rebind or copy its state. */
    as_interface(target) {
        if (!(target instanceof BamlInterfaceType)) {
            throw new TypeError('as_interface expects a generated Ref.type(...) value');
        }
        const typeMap = this.#typeMap;
        target._checkSdk(typeMap);
        const encoded = encodeCallArgs({ value: this }, {
            callId: BigInt(newFunctionCall()), functionName: 'baml.identity',
            typeArgs: [['T', target]], typeMap,
        });
        // Native preparation roots the receiver before this Promise is returned.
        const pending = this.#handle._callOwnedFunction(encoded);
        return pending.then(result => decodeCallResult(result, typeMap));
    }
    _invokeConcrete(className, pattern, member, arguments_, choices, parameters = [], context) {
        return this._invoke(member, arguments_, choices, parameters, context, {
            receiver: this.#handle.key,
            className,
            member,
            ...(pattern === null ? { inherent: true } : {
                interfacePattern: baml_bridge.cffi.v1.BamlTy.decode(Uint8Array.from(pattern)),
            }),
        });
    }
    _invoke(member, arguments_, choices, parameters = [], context, concreteMethod) {
        const names = Object.keys(choices ?? {});
        if (names.length !== parameters.length || names.some(name => !parameters.includes(name))) {
            throw new TypeError(`method $types must specify exactly ${JSON.stringify(parameters)}`);
        }
        const typeMap = this.#typeMap;
        const typeArgs = parameters.map(name => {
            const value = choices[name];
            if (!isBamlType(value))
                throw new TypeError('method $types must contain BamlType values');
            return ['', value];
        });
        const callId = BigInt(newFunctionCall());
        const binding = attachCallContext(context, callId);
        try {
            const encoded = encodeCallArgs(arguments_, { callId, typeArgs, typeMap, concreteMethod });
            // Admission and receiver rooting happen before returning the Promise.
            const pending = concreteMethod
                ? this.#handle._callOwnedFunction(encoded)
                : this.#handle._callInterfaceMethod(member, encoded);
            return pending.then(result => decodeCallResult(result, typeMap))
                .finally(() => binding.detach());
        }
        catch (error) {
            binding.detach();
            throw error;
        }
    }
}
export class BamlInterfaceRef extends BamlRef {
}
const concreteTypeEvidence = Symbol('baml.concrete.type');
export class BamlConcreteRef extends BamlRef {
}
//# sourceMappingURL=interface_ref.js.map