// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test, console} from "forge-std/Test.sol";
import {FlashArbitrage} from "../src/FlashArbitrage.sol";
import {IUniswapV2Pair} from "../src/interfaces/IUniswapV2Pair.sol";
import {IERC20} from "openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";

contract MockERC20 is IERC20 {
    string public name;
    string public symbol;
    uint8 public decimals = 18;
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    constructor(string memory _name, string memory _symbol) {
        name = _name;
        symbol = _symbol;
    }

    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
        totalSupply += amount;
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        return true;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        allowance[from][msg.sender] -= amount;
        balanceOf[from] -= amount;
        balanceOf[to] += amount;
        return true;
    }
}

contract FlashArbitrageTest is Test {
    FlashArbitrage public arbitrage;
    MockERC20 public tokenA;
    MockERC20 public tokenB;

    address public owner = address(this);
    address public notOwner = address(0x1234);

    function setUp() public {
        arbitrage = new FlashArbitrage();
        tokenA = new MockERC20("Token A", "TKA");
        tokenB = new MockERC20("Token B", "TKB");
    }

    function test_owner() public view {
        assertEq(arbitrage.owner(), owner);
    }

    function test_withdraw_onlyOwner() public {
        // Mint tokens to contract
        tokenA.mint(address(arbitrage), 1000e18);

        // Owner can withdraw
        arbitrage.withdraw(address(tokenA));
        assertEq(tokenA.balanceOf(owner), 1000e18);
    }

    function test_withdraw_notOwner_reverts() public {
        tokenA.mint(address(arbitrage), 1000e18);

        vm.prank(notOwner);
        vm.expectRevert(FlashArbitrage.NotOwner.selector);
        arbitrage.withdraw(address(tokenA));
    }

    function test_executeArbitrage_notOwner_reverts() public {
        vm.prank(notOwner);
        vm.expectRevert(FlashArbitrage.NotOwner.selector);
        arbitrage.executeArbitrage(
            address(0x1),
            address(0x2),
            address(tokenA),
            1000e18
        );
    }

    function test_withdrawETH() public {
        // Send ETH to contract
        vm.deal(address(arbitrage), 1 ether);

        uint256 balanceBefore = address(this).balance;
        arbitrage.withdrawETH();
        assertEq(address(this).balance, balanceBefore + 1 ether);
    }

    // Allow this contract to receive ETH
    receive() external payable {}
}
