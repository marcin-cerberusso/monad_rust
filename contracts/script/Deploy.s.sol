// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Script, console} from "forge-std/Script.sol";
import {FlashArbitrage} from "../src/FlashArbitrage.sol";

contract DeployScript is Script {
    function setUp() public {}

    function run() public {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        
        vm.startBroadcast(deployerPrivateKey);
        
        FlashArbitrage arbitrage = new FlashArbitrage();
        
        console.log("FlashArbitrage deployed at:", address(arbitrage));
        console.log("Owner:", arbitrage.owner());
        
        vm.stopBroadcast();
    }
}
