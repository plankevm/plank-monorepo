// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {ERC20 as SoladyERC20} from "solady/tokens/ERC20.sol";

contract ERC20 is SoladyERC20 {
    constructor() {
        _mint(msg.sender, 1000000);
    }

    function name() public pure override returns (string memory) {
        return "Test Token";
    }

    function symbol() public pure override returns (string memory) {
        return "TST";
    }
}
