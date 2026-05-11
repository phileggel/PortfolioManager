use crate::{
    context::{account, asset},
    core::{logger, Event},
    use_cases::{
        account_deletion, account_details, archive_asset, asset_web_lookup, delete_asset,
        holding_transaction, update_checker,
    },
};

/// create the Specta builder for standard and generate_bindings
pub fn create_specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        // ----- asset BC -----
        .typ::<asset::Asset>()
        .typ::<asset::AssetCategory>()
        .typ::<asset::AssetClass>()
        .typ::<asset::AssetApplicationError>()
        .typ::<asset::AssetCrudError>()
        .typ::<asset::AssetDomainError>()
        .typ::<asset::CategoryCrudError>()
        .typ::<asset::CategoryApplicationError>()
        .typ::<asset::AssetPrice>()
        .typ::<asset::AssetPriceDomainError>()
        .typ::<asset::AssetPriceApplicationError>()
        .typ::<asset::AssetPriceError>()
        // ----- account BC -----
        .typ::<account::Account>()
        .typ::<account::UpdateFrequency>()
        .typ::<account::Holding>()
        .typ::<account::AccountCrudError>()
        .typ::<account::Transaction>()
        .typ::<account::TransactionType>()
        .typ::<account::HoldingTransactionError>()
        .typ::<crate::core::InfrastructureError>()
        // ----- use cases -----
        .typ::<archive_asset::ArchiveAssetCommandError>()
        .typ::<delete_asset::DeleteAssetCommandError>()
        .typ::<holding_transaction::BuyHoldingDTO>()
        .typ::<holding_transaction::SellHoldingDTO>()
        .typ::<holding_transaction::CorrectTransactionDTO>()
        .typ::<holding_transaction::OpenHoldingDTO>()
        .typ::<holding_transaction::OpenHoldingError>()
        .typ::<holding_transaction::OpenHoldingApplicationError>()
        .typ::<holding_transaction::DepositDTO>()
        .typ::<holding_transaction::WithdrawalDTO>()
        .typ::<account_details::HoldingDetail>()
        .typ::<account_details::ClosedHoldingDetail>()
        .typ::<account_details::AccountDetailsResponse>()
        .typ::<account_details::AccountDetailsCommandError>()
        .typ::<account_deletion::AccountDeletionSummary>()
        .typ::<account_deletion::AccountDeletionCommandError>()
        .typ::<asset_web_lookup::AssetLookupResult>()
        .typ::<asset_web_lookup::WebLookupCommandError>()
        .typ::<update_checker::UpdateInfo>()
        .commands(tauri_specta::collect_commands![
            // ----- asset BC -----
            asset::get_assets,
            asset::get_assets_with_archived,
            asset::add_asset,
            asset::update_asset,
            asset::unarchive_asset,
            asset::get_categories,
            asset::add_category,
            asset::update_category,
            asset::delete_category,
            asset::record_asset_price,
            asset::get_asset_prices,
            asset::update_asset_price,
            asset::delete_asset_price,
            // ----- account BC -----
            account::get_accounts,
            account::add_account,
            account::update_account,
            account::delete_account,
            account::get_asset_ids_for_account,
            account::get_transactions,
            // ----- use cases -----
            archive_asset::archive_asset,
            delete_asset::delete_asset,
            holding_transaction::open_holding,
            holding_transaction::buy_holding,
            holding_transaction::sell_holding,
            holding_transaction::correct_transaction,
            holding_transaction::cancel_transaction,
            holding_transaction::record_deposit,
            holding_transaction::record_withdrawal,
            account_details::get_account_details,
            account_deletion::get_account_deletion_summary,
            asset_web_lookup::lookup_asset,
            update_checker::check_for_update,
            update_checker::download_update,
            update_checker::install_update,
            // ----- core -----
            logger::log_frontend
        ])
        .events(tauri_specta::collect_events![Event])
}
