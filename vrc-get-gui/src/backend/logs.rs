use crate::activity_log::{
    ActivityEntry, ActivityEntryFilter, ActivityLogContextParams, ActivityLogContextResponse,
    ActivityLogEntryParams, ActivityLogQueryError, ActivityLogSearchParams,
    ActivityLogSearchResponse, ActivityLogState, ActivityLogSummaryParams,
    ActivityLogSummaryResponse,
};
use crate::logging::{
    TechnicalLogEntryDetails, TechnicalLogEntryParams, TechnicalLogQueryError,
    TechnicalLogSearchParams, TechnicalLogSearchResponse, TechnicalLogSummaryParams,
    TechnicalLogSummaryResponse,
};
use std::path::Path;

pub(crate) fn get_activity_entries(
    activity: &ActivityLogState,
    filter: ActivityEntryFilter,
) -> Vec<ActivityEntry> {
    activity.get_entries(filter)
}

pub(crate) fn search_activity_logs(
    activity: &ActivityLogState,
    params: ActivityLogSearchParams,
) -> Result<ActivityLogSearchResponse, ActivityLogQueryError> {
    activity.search_entries(params)
}

pub(crate) fn get_activity_log_entry(
    activity: &ActivityLogState,
    params: ActivityLogEntryParams,
) -> Result<ActivityEntry, ActivityLogQueryError> {
    activity.get_entry(params)
}

pub(crate) fn summarize_activity_logs(
    activity: &ActivityLogState,
    params: ActivityLogSummaryParams,
) -> Result<ActivityLogSummaryResponse, ActivityLogQueryError> {
    activity.summarize_entries(params)
}

pub(crate) fn get_activity_log_context(
    activity: &ActivityLogState,
    params: ActivityLogContextParams,
) -> Result<ActivityLogContextResponse, ActivityLogQueryError> {
    activity.entry_context(params)
}

pub(crate) fn search_technical_logs(
    log_folder: &Path,
    params: TechnicalLogSearchParams,
) -> Result<TechnicalLogSearchResponse, TechnicalLogQueryError> {
    crate::logging::search_technical_logs(log_folder, params)
}

pub(crate) fn get_technical_log_entry(
    log_folder: &Path,
    params: TechnicalLogEntryParams,
) -> Result<TechnicalLogEntryDetails, TechnicalLogQueryError> {
    crate::logging::get_technical_log_entry(log_folder, params)
}

pub(crate) fn summarize_technical_logs(
    log_folder: &Path,
    params: TechnicalLogSummaryParams,
) -> Result<TechnicalLogSummaryResponse, TechnicalLogQueryError> {
    crate::logging::summarize_technical_logs(log_folder, params)
}
