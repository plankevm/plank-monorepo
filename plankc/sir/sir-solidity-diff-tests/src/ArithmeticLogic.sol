// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @author philogy <https://github.com/philogy>
contract ArithmeticLogic {
    // ========== EVM Arithmetic ==========

    function add_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := add(a, b)
        }
    }

    function mul_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := mul(a, b)
        }
    }

    function sub_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := sub(a, b)
        }
    }

    function div_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := div(a, b)
        }
    }

    function sdiv_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := sdiv(a, b)
        }
    }

    function mod_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := mod(a, b)
        }
    }

    function smod_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := smod(a, b)
        }
    }

    function addmod_(uint256 a, uint256 b, uint256 n) external pure returns (uint256 result) {
        assembly {
            result := addmod(a, b, n)
        }
    }

    function mulmod_(uint256 a, uint256 b, uint256 n) external pure returns (uint256 result) {
        assembly {
            result := mulmod(a, b, n)
        }
    }

    function exp_(uint256 a, uint256 exponent) external pure returns (uint256 result) {
        assembly {
            result := exp(a, exponent)
        }
    }

    function signextend_(uint256 b, uint256 x) external pure returns (uint256 result) {
        assembly {
            result := signextend(b, x)
        }
    }

    // ========== EVM Comparison & Bitwise Logic ==========

    function lt_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := lt(a, b)
        }
    }

    function gt_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := gt(a, b)
        }
    }

    function slt_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := slt(a, b)
        }
    }

    function sgt_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := sgt(a, b)
        }
    }

    function eq_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := eq(a, b)
        }
    }

    function iszero_(uint256 a) external pure returns (uint256 result) {
        assembly {
            result := iszero(a)
        }
    }

    function and_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := and(a, b)
        }
    }

    function or_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := or(a, b)
        }
    }

    function xor_(uint256 a, uint256 b) external pure returns (uint256 result) {
        assembly {
            result := xor(a, b)
        }
    }

    function not_(uint256 a) external pure returns (uint256 result) {
        assembly {
            result := not(a)
        }
    }

    function byte_(uint256 i, uint256 x) external pure returns (uint256 result) {
        assembly {
            result := byte(i, x)
        }
    }

    function shl_(uint256 shift, uint256 value) external pure returns (uint256 result) {
        assembly {
            result := shl(shift, value)
        }
    }

    function shr_(uint256 shift, uint256 value) external pure returns (uint256 result) {
        assembly {
            result := shr(shift, value)
        }
    }

    function sar_(uint256 shift, uint256 value) external pure returns (uint256 result) {
        assembly {
            result := sar(shift, value)
        }
    }
}
