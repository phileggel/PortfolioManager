import type {
  AccountApplicationError,
  AssetPriceError,
  BuyHoldingDTO,
  CorrectTransactionDTO,
  HoldingTransactionError,
  SellHoldingDTO,
  Transaction,
} from "../../bindings";
import { commands, type Result } from "../../bindings";

/**
 * Gateway for Transaction-related backend communication.
 * Centralizes all Tauri command calls for the Transaction feature.
 */
export const transactionGateway = {
  async buyHolding(dto: BuyHoldingDTO): Promise<Result<Transaction, HoldingTransactionError>> {
    return await commands.buyHolding(dto);
  },

  async sellHolding(dto: SellHoldingDTO): Promise<Result<Transaction, HoldingTransactionError>> {
    return await commands.sellHolding(dto);
  },

  async correctTransaction(
    id: string,
    accountId: string,
    dto: CorrectTransactionDTO,
  ): Promise<Result<Transaction, HoldingTransactionError>> {
    return await commands.correctTransaction(id, accountId, dto);
  },

  async cancelTransaction(
    id: string,
    accountId: string,
  ): Promise<Result<null, HoldingTransactionError>> {
    return await commands.cancelTransaction(id, accountId);
  },

  async getTransactions(
    accountId: string,
    assetId: string,
  ): Promise<Result<Transaction[], AccountApplicationError>> {
    return await commands.getTransactions(accountId, assetId);
  },

  async getAssetIdsForAccount(
    accountId: string,
  ): Promise<Result<string[], AccountApplicationError>> {
    return await commands.getAssetIdsForAccount(accountId);
  },

  async recordAssetPrice(
    assetId: string,
    date: string,
    price: number,
  ): Promise<Result<null, AssetPriceError>> {
    return await commands.recordAssetPrice(assetId, date, price);
  },
};
