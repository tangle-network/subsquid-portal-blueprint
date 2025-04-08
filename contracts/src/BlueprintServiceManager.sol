// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

import "tnt-core/BlueprintServiceManagerBase.sol";
import "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "./SimplePriceOracle.sol";

/// @title Subsquid Portal Blueprint Service Manager
/// @author Your Name / Team
/// @notice Manages the lifecycle and job execution for the Subsquid Portal Blueprint.
/// Includes specific logic for handling API key generation requests with payment verification.
contract BlueprintServiceManager is BlueprintServiceManagerBase, Ownable {
    using EnumerableSet for EnumerableSet.AddressSet;

    // --- Constants --- 
    // Job IDs defined in the Rust blueprint lib (ensure these match)
    uint8 public constant JOB_ID_PROVISION_PORTAL = 1;
    uint8 public constant JOB_ID_CONFIGURE_GATEWAY = 2;
    uint8 public constant JOB_ID_GENERATE_API_KEY = 3;

    // --- Oracle and Payment Configuration --- 
    address public simplePriceOracleAddress;

    // The denominator for price feeds (e.g., ETH/USD feed might have 8 decimals)
    uint256 public constant PRICE_FEED_DECIMALS = 8;
    // The required payment amount in USD (e.g., $1 for an API key = 1 * 10**8)
    uint256 public apiKeyPriceUsd = 1 * (10**PRICE_FEED_DECIMALS);

    // --- Events --- 
    event ApiKeyRequested(uint64 indexed serviceId, address indexed requester, uint64 jobCallId);
    event PaymentVerified(uint64 indexed jobCallId, address token, uint256 amount);
    event OracleAddressSet(address indexed token, address indexed feed);
    event ApiKeyPriceSet(uint256 newPriceUsd);

    // --- Constructor --- 
    constructor(address initialOwner) Ownable(initialOwner) {}

    // --- Configuration Functions (Owner Only) --- 

    // Add a function for the owner to set this address
    function setSimplePriceOracle(address _oracleAddress) external onlyOwner {
        require(_oracleAddress != address(0), "Invalid oracle address");
        simplePriceOracleAddress = _oracleAddress;
    }

    /// @notice Sets the price in USD for generating an API key.
    /// @param _priceUsd The price in USD, scaled by 10**PRICE_FEED_DECIMALS (e.g., $1 = 100000000).
    function setApiKeyPrice(uint256 _priceUsd) external onlyOwner {
        apiKeyPriceUsd = _priceUsd;
        emit ApiKeyPriceSet(_priceUsd);
    }

    // --- Job Call Hook --- 

    function payForApiKey(address token, address requester) external payable {
        // Get the oracle price
        uint256 requiredAmount = getRequiredTokenAmount(token);
        // Transfer the tokens from the requester to this contract
        ERC20 tokenContract = ERC20(token);
        require(tokenContract.transferFrom(requester, address(this), requiredAmount), "ERC20 transfer failed");
        // Emit an event
        emit PaymentVerified(JOB_ID_GENERATE_API_KEY, token, requiredAmount);

        // Call the runtime to submit a job call
        submitJobCall(JOB_ID_GENERATE_API_KEY, requester, token, requiredAmount);
    }

    function submitJobCall(uint8 jobId, address requester, address token, uint256 amount) internal {
        // TODO: Implement job call submission
    }

    // --- Internal Helper Functions --- 

    /// @dev Calculates the required ERC20 token amount based on the USD price.
    function getRequiredTokenAmount(address tokenAddress) internal view returns (uint256) {
        require(simplePriceOracleAddress != address(0), "Oracle address not set");
        SimplePriceOracle oracle = SimplePriceOracle(simplePriceOracleAddress);

        // Create assetId, e.g., keccak256(abi.encodePacked(ERC20(tokenAddress).symbol(), "/USD"))
        // This needs a reliable way to map token address to asset ID used in the oracle
        bytes32 assetId = keccak256(abi.encodePacked(ERC20(tokenAddress).symbol(), "/USD")); // Placeholder ID

        uint8 priceDecimals = oracle.getDecimals(assetId);
        require(priceDecimals > 0, "Oracle decimals not set for asset");

        (uint256 price, uint256 updatedAt) = oracle.getLatestPrice(assetId);
        require(price > 0, "Invalid oracle price");
        // Optional: Check freshness using updatedAt if needed:
        // require(block.timestamp - updatedAt < STALENESS_THRESHOLD, "Oracle price stale");

        ERC20 tokenContract = ERC20(tokenAddress);
        uint8 tokenDecimals = tokenContract.decimals();

        // Calculation remains similar, but uses price and decimals from SimplePriceOracle
        uint256 requiredAmount = (apiKeyPriceUsd * (10**tokenDecimals) * (10**priceDecimals)) / (price * (10**PRICE_FEED_DECIMALS));

        return requiredAmount;
    }

     // --- Fallback Function --- 
     // receive() external payable {}

     // --- Owner Functions --- 
     function withdrawTokens(address tokenAddress, address to, uint256 amount) external onlyOwner {
         ERC20 tokenContract = ERC20(tokenAddress);
         require(tokenContract.transfer(to, amount), "Withdrawal failed");
     }
} 