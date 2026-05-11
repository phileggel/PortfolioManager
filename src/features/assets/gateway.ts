import {
  type ArchiveAssetError,
  type Asset,
  type AssetApplicationError,
  type AssetCrudError,
  type AssetLookupResult,
  type CreateAssetDTO,
  commands,
  type DeleteAssetError,
  type Result,
  type UpdateAssetDTO,
  type WebLookupCommandError,
} from "../../bindings";

/**
 * Gateway for Asset-related backend communication.
 * Centralizes all Tauri command calls for the Asset feature.
 */
export const assetGateway = {
  async getAssets(): Promise<Result<Asset[], AssetApplicationError>> {
    return await commands.getAssets();
  },

  async getAssetsWithArchived(): Promise<Result<Asset[], AssetApplicationError>> {
    return await commands.getAssetsWithArchived();
  },

  async createAsset(dto: CreateAssetDTO): Promise<Result<Asset, AssetCrudError>> {
    return await commands.addAsset(dto);
  },

  async updateAsset(dto: UpdateAssetDTO): Promise<Result<Asset, AssetCrudError>> {
    return await commands.updateAsset(dto);
  },

  async archiveAsset(id: string): Promise<Result<null, ArchiveAssetError>> {
    return await commands.archiveAsset(id);
  },

  async unarchiveAsset(id: string): Promise<Result<null, AssetCrudError>> {
    return await commands.unarchiveAsset(id);
  },

  async deleteAsset(id: string): Promise<Result<null, DeleteAssetError>> {
    return await commands.deleteAsset(id);
  },

  async lookupAsset(query: string): Promise<Result<AssetLookupResult[], WebLookupCommandError>> {
    return await commands.lookupAsset(query);
  },
};
