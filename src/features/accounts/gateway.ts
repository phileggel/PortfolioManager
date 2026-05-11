import {
  type Account,
  type AccountApplicationError,
  type AccountCrudError,
  type AccountDeletionSummary,
  type CreateAccountDTO,
  commands,
  type InfrastructureError,
  type Result,
  type UpdateAccountDTO,
} from "../../bindings";

/**
 * Gateway for Account-related backend communication.
 * Centralizes all Tauri command calls for the Account feature.
 */
export const accountGateway = {
  async getAccounts(): Promise<Result<Account[], InfrastructureError>> {
    return await commands.getAccounts();
  },

  async addAccount(dto: CreateAccountDTO): Promise<Result<Account, AccountCrudError>> {
    return await commands.addAccount(dto);
  },

  async updateAccount(dto: UpdateAccountDTO): Promise<Result<Account, AccountCrudError>> {
    return await commands.updateAccount(dto);
  },

  async deleteAccount(id: string): Promise<Result<null, InfrastructureError>> {
    return await commands.deleteAccount(id);
  },

  async getAccountDeletionSummary(
    accountId: string,
  ): Promise<Result<AccountDeletionSummary, AccountApplicationError>> {
    return await commands.getAccountDeletionSummary(accountId);
  },
};
