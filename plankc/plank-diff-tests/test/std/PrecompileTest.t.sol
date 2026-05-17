// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {console} from "forge-std/console.sol";

contract PrecompileTestTest is BaseTest {
    address plankImpl = makeAddr("plank-implementation");

    function setUp() public {
        bytes memory plankCode = plank("src/std/precompile_test.plk");
        plankImpl = deployCode(plankCode);
    }

    function test_ecrecover() public {
        // known good values from https://www.evm.codes/precompiled
        bytes32 hash = 0x456e9aea5e197a1f1af7a3e85a3212fa4049a3ba34c2289b4c860fc0b0c64ef3;
        uint8 v = 28;
        bytes32 r = 0x9242685bf161793cc25603c231bc2f568eb630ea16aa137d2664ac8038825608;
        bytes32 s = 0x4f8ae3bd7535248d0bd448298cc2e2071e56992d0774dc340c368ae950852ada;

        (bool succ, bytes memory out) =
            plankImpl.call(abi.encodeWithSignature("ecrecoverExt(bytes32,uint8,bytes32,bytes32)", hash, v, r, s));

        assert(succ);
        assertEq(out.length, 32);
        assertEq(bytes32(out), 0x0000000000000000000000007156526fbd7a3c72969b54f64e42c10fbb768c8a);
        assertEq(bytes32(out), bytes32(uint256(uint160(ecrecover(hash, v, r, s)))));
    }

    function test_ecrecover_invalid() public {
        (bool succ, bytes memory out) = plankImpl.call(
            abi.encodeWithSignature(
                "ecrecoverExt(bytes32,uint8,bytes32,bytes32)", bytes32(0), 0, bytes32(0), bytes32(0)
            )
        );

        assert(succ);
        assertEq(out.length, 32);
        assertEq(bytes32(out), bytes32(0));
        assertEq(bytes32(out), bytes32(uint256(uint160(ecrecover(0, 0, 0, 0)))));
    }

    function test_sha256() public {
        bytes memory data = abi.encode(0xdeadbeef);
        (bool succ, bytes memory out) = plankImpl.call(abi.encodeWithSignature("sha256Ext(bytes)", data));
        assertEq(succ, true);
        assertEq(bytes32(out), sha256(data));
    }

    function test_fuzz_sha256(bytes memory data) public {
        (bool succ, bytes memory out) = plankImpl.call(abi.encodeWithSignature("sha256Ext(bytes)", data));
        assertEq(succ, true);
        assertEq(bytes32(out), sha256(data));
    }

    function test_ripemd160() public {
        bytes memory data = abi.encode(0xdeadbeef);
        (bool succ, bytes memory out) = plankImpl.call(abi.encodeWithSignature("ripemd160Ext(bytes)", data));
        assertEq(succ, true);
        assertEq(bytes32(out), bytes32(uint256(uint160(ripemd160(data)))));
    }

    function test_fuzz_ripemd160(bytes memory data) public {
        (bool succ, bytes memory out) = plankImpl.call(abi.encodeWithSignature("ripemd160Ext(bytes)", data));
        assertEq(succ, true);
        assertEq(bytes32(out), bytes32(uint256(uint160(ripemd160(data)))));
    }

    function test_modexp() public {
        bytes memory base = abi.encode(0x02);
        bytes memory exp = abi.encode(0x02);
        bytes memory mod = abi.encode(0xff);

        (bool succ, bytes memory out) =
            plankImpl.call(abi.encodeWithSignature("modexpExt(bytes,bytes,bytes)", base, exp, mod));

        assertTrue(succ);
        assertEq(abi.decode(out, (bytes)), modexp(base, exp, mod));
    }

    function test_fuzz_modexp(bytes memory base, bytes memory exp, bytes memory mod) public {
        (bool succ, bytes memory out) =
            plankImpl.call(abi.encodeWithSignature("modexpExt(bytes,bytes,bytes)", base, exp, mod));

        assertTrue(succ);
        assertEq(abi.decode(out, (bytes)), modexp(base, exp, mod));
    }

    function modexp(bytes memory base, bytes memory exp, bytes memory mod) internal view returns (bytes memory) {
        (bool success, bytes memory out) = address(5)
            .staticcall(bytes.concat(bytes32(base.length), bytes32(exp.length), bytes32(mod.length), base, exp, mod));

        assert(success);
        return out;
    }
}
