// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {EventTest} from "../../src/std/EventTest.sol";

contract EventDiffTest is BaseTest {
    // Every `emit` / `emit_anonymous` dispatch arm (LOG0 through LOG4), every
    // supported field type indexed and not, empty and mixed static/dynamic
    // data sections, and `bytes` length edge cases.
    function test_eventsMatchSolidity() public {
        address ref = address(new EventTest());
        address impl = plankDeploy("src/std/event_test.plk");
        assertCallEq(ref, impl, "");
    }

    // Compiling this file runs its comptime_assert block. Without this test
    // nothing in CI ever builds it, so the signature and topic0 assertions
    // from Tasks 1-4 would silently never run.
    function test_comptimeAssertions() public {
        plankDeploy("src/std/event_comptime_test.plk");
    }
}
