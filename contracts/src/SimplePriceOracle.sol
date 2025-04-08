// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/access/Ownable.sol";

/// @title SimplePriceOracle
/// @author Tangle Network Team
/// @notice A basic oracle contract where an owner can manually update prices for assets.
/// This contract does *not* connect to external oracles like Chainlink directly.
/// The owner is responsible for fetching data off-chain and updating it here.
contract SimplePriceOracle is Ownable {

    /// @notice Stores the latest price data for an asset identifier.
    struct PriceData {
        uint256 price;     // The price, scaled according to the asset's decimals.
        uint256 updatedAt; // Timestamp of the last update.
    }

    // Mapping from an asset identifier (e.g., keccak256("ETH/USD")) to its price data.
    mapping(bytes32 => PriceData) private latestPrices;
    // Mapping from an asset identifier to its price feed's decimal places (e.g., 8 for USD pairs).
    mapping(bytes32 => uint8) public decimals;

    /// @notice Emitted when an asset's price is updated.
    event PriceUpdated(bytes32 indexed assetId, uint256 price, uint256 timestamp);
    /// @notice Emitted when the decimals for an asset identifier are set.
    event DecimalsSet(bytes32 indexed assetId, uint8 decimals);

    /// @notice Initializes the contract, setting the deployer as the initial owner.
    /// @param initialOwner The address designated as the initial owner.
    constructor(address initialOwner) Ownable(initialOwner) {}

    /// @notice Updates the price for a given asset identifier.
    /// @dev Can only be called by the owner.
    /// @param assetId The unique identifier for the asset pair (e.g., keccak256("ETH/USD")).
    /// @param _price The new price for the asset, scaled according to its decimals.
    function updatePrice(bytes32 assetId, uint256 _price) external onlyOwner {
        require(decimals[assetId] > 0, "Decimals not set for asset"); // Ensure decimals are configured before price
        latestPrices[assetId] = PriceData({
            price: _price,
            updatedAt: block.timestamp
        });
        emit PriceUpdated(assetId, _price, block.timestamp);
    }

    /// @notice Sets the number of decimal places for a given asset identifier's price feed.
    /// @dev Can only be called by the owner. This is crucial for interpreting the price correctly.
    /// @param assetId The unique identifier for the asset pair.
    /// @param _decimals The number of decimal places (e.g., 8 for USD pairs from Chainlink).
    function setDecimals(bytes32 assetId, uint8 _decimals) external onlyOwner {
        require(_decimals > 0 && _decimals <= 18, "Decimals must be between 1 and 18"); // Sensible range
        decimals[assetId] = _decimals;
        emit DecimalsSet(assetId, _decimals);
    }

    /// @notice Retrieves the latest price data for a given asset identifier.
    /// @param assetId The unique identifier for the asset pair.
    /// @return price The latest price.
    /// @return updatedAt The timestamp of the last update.
    function getLatestPrice(bytes32 assetId) external view returns (uint256 price, uint256 updatedAt) {
        PriceData memory data = latestPrices[assetId];
        // Return 0 if price was never updated or decimals aren't set
        if (data.updatedAt == 0 || decimals[assetId] == 0) {
            return (0, 0);
        }
        return (data.price, data.updatedAt);
    }

    /// @notice Retrieves the configured number of decimals for an asset identifier.
    /// @param assetId The unique identifier for the asset pair.
    /// @return uint8 The number of decimal places.
    function getDecimals(bytes32 assetId) external view returns (uint8) {
        return decimals[assetId];
    }
}
