// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IUniswapV2Callee} from "./interfaces/IUniswapV2Callee.sol";
import {IUniswapV2Pair} from "./interfaces/IUniswapV2Pair.sol";
import {IERC20} from "openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "openzeppelin-contracts/contracts/token/ERC20/utils/SafeERC20.sol";

/// @title FlashArbitrage
/// @notice Atomic DEX-DEX arbitrage using Uniswap V2 flash swaps
/// @dev Borrows tokens from pairA, swaps on pairB, repays pairA, keeps profit
contract FlashArbitrage is IUniswapV2Callee {
    using SafeERC20 for IERC20;

    address public immutable owner;

    error NotOwner();
    error NotPair();
    error InsufficientProfit();

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    constructor() {
        owner = msg.sender;
    }

    /// @notice Initiate atomic arbitrage between two Uniswap V2 pairs
    /// @param pairA The pair to flash swap from (borrow tokens)
    /// @param pairB The pair to swap on for profit
    /// @param tokenIn The token to borrow and trade
    /// @param amountIn Amount to borrow from pairA
    function executeArbitrage(
        address pairA,
        address pairB,
        address tokenIn,
        uint256 amountIn
    ) external onlyOwner {
        // Determine which token slot tokenIn occupies in pairA
        address token0 = IUniswapV2Pair(pairA).token0();
        
        uint256 amount0Out;
        uint256 amount1Out;
        
        if (tokenIn == token0) {
            amount0Out = amountIn;
            amount1Out = 0;
        } else {
            amount0Out = 0;
            amount1Out = amountIn;
        }

        // Encode arbitrage params for callback
        bytes memory data = abi.encode(pairA, pairB, tokenIn, amountIn);

        // Initiate flash swap - pairA will call uniswapV2Call
        IUniswapV2Pair(pairA).swap(amount0Out, amount1Out, address(this), data);
    }

    /// @notice Callback from Uniswap V2 flash swap
    /// @dev Execute arbitrage and repay loan with profit
    function uniswapV2Call(
        address sender,
        uint256 amount0,
        uint256 amount1,
        bytes calldata data
    ) external override {
        // Decode params
        (address pairA, address pairB, address tokenIn, uint256 amountIn) = 
            abi.decode(data, (address, address, address, uint256));

        // Security: ensure callback is from pairA
        if (msg.sender != pairA) revert NotPair();

        // Get the borrowed amount
        uint256 borrowedAmount = amount0 > 0 ? amount0 : amount1;

        // Calculate repayment amount (0.3% fee)
        // repayAmount = borrowedAmount * 1000 / 997 + 1 (round up)
        uint256 repayAmount = (borrowedAmount * 1000) / 997 + 1;

        // Get tokenOut from pairB
        address token0B = IUniswapV2Pair(pairB).token0();
        address token1B = IUniswapV2Pair(pairB).token1();
        address tokenOut = tokenIn == token0B ? token1B : token0B;

        // Execute swap on pairB: tokenIn -> tokenOut
        uint256 amountOut = _swapOnPair(pairB, tokenIn, borrowedAmount);

        // Now swap tokenOut back to tokenIn on pairB or another path
        // For simplicity, we assume pairB gives us tokenIn back directly
        // In production, you'd want multi-hop support

        // Repay pairA
        IERC20(tokenIn).safeTransfer(pairA, repayAmount);

        // Check profit
        uint256 profit = IERC20(tokenIn).balanceOf(address(this));
        if (profit == 0) revert InsufficientProfit();

        // Transfer profit to owner
        IERC20(tokenIn).safeTransfer(owner, profit);
    }

    /// @notice Swap tokens on a Uniswap V2 pair
    /// @param pair The pair to swap on
    /// @param tokenIn Token to sell
    /// @param amountIn Amount to sell
    /// @return amountOut Amount received
    function _swapOnPair(
        address pair,
        address tokenIn,
        uint256 amountIn
    ) internal returns (uint256 amountOut) {
        address token0 = IUniswapV2Pair(pair).token0();
        address token1 = IUniswapV2Pair(pair).token1();
        
        (uint112 reserve0, uint112 reserve1,) = IUniswapV2Pair(pair).getReserves();

        bool isToken0 = tokenIn == token0;
        (uint112 reserveIn, uint112 reserveOut) = isToken0 
            ? (reserve0, reserve1) 
            : (reserve1, reserve0);

        // Calculate amountOut using Uniswap V2 formula
        // amountOut = (amountIn * 997 * reserveOut) / (reserveIn * 1000 + amountIn * 997)
        uint256 amountInWithFee = amountIn * 997;
        amountOut = (amountInWithFee * reserveOut) / (uint256(reserveIn) * 1000 + amountInWithFee);

        // Transfer tokenIn to pair
        IERC20(tokenIn).safeTransfer(pair, amountIn);

        // Execute swap
        (uint256 out0, uint256 out1) = isToken0 
            ? (uint256(0), amountOut) 
            : (amountOut, uint256(0));
        
        IUniswapV2Pair(pair).swap(out0, out1, address(this), "");
    }

    /// @notice Withdraw tokens from contract (owner only)
    /// @param token Token to withdraw
    function withdraw(address token) external onlyOwner {
        uint256 balance = IERC20(token).balanceOf(address(this));
        if (balance > 0) {
            IERC20(token).safeTransfer(owner, balance);
        }
    }

    /// @notice Withdraw ETH from contract (owner only)
    function withdrawETH() external onlyOwner {
        uint256 balance = address(this).balance;
        if (balance > 0) {
            payable(owner).transfer(balance);
        }
    }

    receive() external payable {}
}
