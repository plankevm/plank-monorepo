// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

contract PrecompileTest {
    function ecrecoverExt(
        bytes32 hash,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external pure returns (address) {
        return ecrecover(hash, v, r, s);
    }

    function sha256Ext(bytes memory data) external pure returns (bytes32) {
        return sha256(data);
    }

    function ripemd160Ext(bytes memory data) external pure returns (bytes32) {
        bytes20 digest = ripemd160(data);

        // force left padding
        return bytes32(uint256(uint160(digest)));
    }

    function setReturnDataExt(
        bytes memory data
    ) external pure returns (bytes memory) {
        return data;
    }

    function modexpExt(
        bytes memory base,
        bytes memory exp,
        bytes memory mod
    ) public view returns (bytes memory) {
        (bool success, bytes memory out) = address(5).staticcall(
            bytes.concat(
                bytes32(base.length),
                bytes32(exp.length),
                bytes32(mod.length),
                base,
                exp,
                mod
            )
        );
        require(success);

        return out;
    }
}
