import {
  type AssetCategory,
  type CategoryApplicationError,
  type CategoryCrudError,
  commands,
  type Result,
} from "@/bindings";

export const categoryGateway = {
  async getCategories(): Promise<Result<AssetCategory[], CategoryApplicationError>> {
    return commands.getCategories();
  },

  async addCategory(label: string): Promise<Result<AssetCategory, CategoryCrudError>> {
    return commands.addCategory(label);
  },

  async updateCategory(
    id: string,
    label: string,
  ): Promise<Result<AssetCategory, CategoryCrudError>> {
    return commands.updateCategory(id, label);
  },

  async deleteCategory(id: string): Promise<Result<null, CategoryCrudError>> {
    return commands.deleteCategory(id);
  },
};
