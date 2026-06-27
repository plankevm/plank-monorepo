// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

contract Bit {
    fallback() external payable {
        uint256 x = abi.decode(msg.data, (uint256));
        uint256 r = clz(x);
        bytes memory out = abi.encode(r);
        assembly ("memory-safe") {
            return(add(out, 0x20), mload(out))
        }
    }

    function clz(uint256 x) internal pure returns (uint256) {
        if (x == 0) return 256;
        uint256 r = 0;
        while (x & (uint256(1) << 255) == 0) {
            x <<= 1;
            r++;
        }
        return r;
    }
}
