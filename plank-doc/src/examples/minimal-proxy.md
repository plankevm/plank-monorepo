# Minimal Proxy (ERC1167)

A factory contract that deploys cheap clones of an existing contract. Each clone delegates all calls to the same implementation, significantly reducing deployment cost.

Beyond the ERC20 example, this demonstrates:

- Raw bytecode construction: building ERC1167 proxy bytecode in memory
- Contract deployment using `create` and `create2`
- Comptime generics: a single `deploy_clone` function that works with or without a salt

```plank
{{#include ../../../plankc/plank-diff-tests/src/examples/minimal_proxy.plk}}
```

## Dispatch

The contract exposes two entry points for proxy deployment:

- `clone` - deploys a proxy at a non-deterministic address using `create`
- `clone_deterministic` - deploys a proxy at a deterministic address using `create2`

Both read the implementation address from calldata and delegate to `deploy_clone`.

## Building the Proxy Bytecode

The ERC1167 minimal proxy is a fixed 55-byte contract assembled directly in memory:

```plank
let len = 32 + 20 + 15;
let buf = @malloc_uninit(len);
@mstore32(buf +% (len - 32), 0x5af43d82803e903d91602b57fd5bf3);
@mstore32(buf +% (len - 32 - 15), address);
@mstore32(buf +% (len - 32 - 15 - 20), 0x3d602d80600a3d3981f3363d3d373d3d3d363d73);
```

The bytecode consists of three parts: a fixed prefix (20 bytes), the implementation address (20 bytes), and a fixed suffix (15 bytes).
We write the components in backwards order as this allows us to use more
efficient `@mstore32` calls instead of `@mstore20` / `@mstore15` that aren't native in the EVM and therefore cost more.

Such optimizations will be handled automatically by the standard library in the
future.

## Comptime Generic Dispatch

Both `clone` and `clone_deterministic` share the same `deploy_clone` helper. The difference is the comptime type argument passed to it: `clone` uses `void`, meaning no salt specialization is required, while `clone_deterministic` uses `u256` for the salt type.

```plank
const clone = fn () never {
    deploy_clone(void, {});
};

const clone_deterministic = fn () never {
    let salt = @evm_calldataload(36);
    deploy_clone(u256, salt);
};
```

The helper takes a comptime type `SaltT` (`void` or `u256`) and a value of that type. Because `SaltT` is comptime, the compiler specializes each branch and removes runtime checks, allowing shared logic to be written without any runtime branching cost.

In theory, `deploy_clone` could be called with any type, e.g., `deploy_clone(bool, some_value_of_type_bool)`. However, this does not make sense in the context of the proxy. To restrict usages, any type other than `void` and `u256` triggers a compile-time error via the `else` branch:

```plank
const deploy_clone = fn (comptime SaltT: type, salt: SaltT) never {
    let address = if SaltT == void {
        @evm_create(0, buf, 55)
    } else if SaltT == u256 {
        @evm_create2(0, buf, 55, salt)
    } else {
        // compile-time error.
        let _unsupported_clone_type_error: u256 = true;
    };
};
```
