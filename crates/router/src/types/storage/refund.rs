use api_models::payments::AmountFilter;
use async_bb8_diesel::AsyncRunQueryDsl;
use common_utils::errors::CustomResult;
use diesel::{associations::HasTable, BoolExpressionMethods, ExpressionMethods, QueryDsl};
#[cfg(feature = "v1")]
use diesel_models::schema::refund::dsl;
#[cfg(feature = "v2")]
use diesel_models::schema_v2::refund::dsl;
use diesel_models::{
    enums::{Currency, RefundStatus},
    errors,
    query::generics::db_metrics,
    refund::Refund,
};
use error_stack::ResultExt;
use hyperswitch_domain_models::refunds;

use crate::{connection::PgPooledConn, logger};

#[async_trait::async_trait]
pub trait RefundDbExt: Sized {
    #[cfg(feature = "v1")]
    async fn filter_by_constraints(
        conn: &PgPooledConn,
        merchant_id: &common_utils::id_type::MerchantId,
        refund_list_details: &refunds::RefundListConstraints,
        limit: i64,
        offset: i64,
    ) -> CustomResult<Vec<Self>, errors::DatabaseError>;

    #[cfg(feature = "v2")]
    async fn filter_by_constraints(
        conn: &PgPooledConn,
        merchant_id: &common_utils::id_type::MerchantId,
        refund_list_details: refunds::RefundListConstraints,
        limit: i64,
        offset: i64,
    ) -> CustomResult<Vec<Self>, errors::DatabaseError>;

    #[cfg(feature = "v1")]
    async fn filter_by_meta_constraints(
        conn: &PgPooledConn,
        merchant_id: &common_utils::id_type::MerchantId,
        refund_list_details: &common_utils::types::TimeRange,
    ) -> CustomResult<api_models::refunds::RefundListMetaData, errors::DatabaseError>;

    #[cfg(feature = "v1")]
    async fn get_refunds_count(
        conn: &PgPooledConn,
        merchant_id: &common_utils::id_type::MerchantId,
        refund_list_details: &refunds::RefundListConstraints,
    ) -> CustomResult<i64, errors::DatabaseError>;

    #[cfg(feature = "v1")]
    async fn get_refund_status_with_count(
        conn: &PgPooledConn,
        merchant_id: &common_utils::id_type::MerchantId,
        profile_id_list: Option<Vec<common_utils::id_type::ProfileId>>,
        time_range: &common_utils::types::TimeRange,
    ) -> CustomResult<Vec<(RefundStatus, i64)>, errors::DatabaseError>;

    #[cfg(feature = "v2")]
    async fn get_refunds_count(
        conn: &PgPooledConn,
        merchant_id: &common_utils::id_type::MerchantId,
        refund_list_details: refunds::RefundListConstraints,
    ) -> CustomResult<i64, errors::DatabaseError>;
}

#[async_trait::async_trait]
impl RefundDbExt for Refund {
    #[cfg(feature = "v1")]
    async fn filter_by_constraints(
        conn: &PgPooledConn,
        merchant_id: &common_utils::id_type::MerchantId,
        refund_list_details: &refunds::RefundListConstraints,
        limit: i64,
        offset: i64,
    ) -> CustomResult<Vec<Self>, errors::DatabaseError> {
        let mut filter = <Self as HasTable>::table()
            .filter(dsl::merchant_id.eq(merchant_id.to_owned()))
            .order(dsl::modified_at.desc())
            .into_boxed();
        let mut search_by_pay_or_ref_id = false;

        if let (Some(pid), Some(ref_id)) = (
            &refund_list_details.payment_id,
            &refund_list_details.refund_id,
        ) {
            search_by_pay_or_ref_id = true;
            filter = filter
                .filter(
                    dsl::payment_id
                        .eq(pid.to_owned())
                        .or(dsl::refund_id.eq(ref_id.to_owned())),
                )
                .limit(limit)
                .offset(offset);
        };

        if !search_by_pay_or_ref_id {
            match &refund_list_details.payment_id {
                Some(pid) => {
                    filter = filter.filter(dsl::payment_id.eq(pid.to_owned()));
                }
                None => {
                    filter = filter.limit(limit).offset(offset);
                }
            };
        }
        if !search_by_pay_or_ref_id {
            match &refund_list_details.refund_id {
                Some(ref_id) => {
                    filter = filter.filter(dsl::refund_id.eq(ref_id.to_owned()));
                }
                None => {
                    filter = filter.limit(limit).offset(offset);
                }
            };
        }
        match &refund_list_details.profile_id {
            Some(profile_id) => {
                filter = filter
                    .filter(dsl::profile_id.eq_any(profile_id.to_owned()))
                    .limit(limit)
                    .offset(offset);
            }
            None => {
                filter = filter.limit(limit).offset(offset);
            }
        };

        if let Some(time_range) = refund_list_details.time_range {
            filter = filter.filter(dsl::created_at.ge(time_range.start_time));

            if let Some(end_time) = time_range.end_time {
                filter = filter.filter(dsl::created_at.le(end_time));
            }
        }

        filter = match refund_list_details.amount_filter {
            Some(AmountFilter {
                start_amount: Some(start),
                end_amount: Some(end),
            }) => filter.filter(dsl::refund_amount.between(start, end)),
            Some(AmountFilter {
                start_amount: Some(start),
                end_amount: None,
            }) => filter.filter(dsl::refund_amount.ge(start)),
            Some(AmountFilter {
                start_amount: None,
                end_amount: Some(end),
            }) => filter.filter(dsl::refund_amount.le(end)),
            _ => filter,
        };

        if let Some(connector) = refund_list_details.connector.clone() {
            filter = filter.filter(dsl::connector.eq_any(connector));
        }

        if let Some(merchant_connector_id) = refund_list_details.merchant_connector_id.clone() {
            filter = filter.filter(dsl::merchant_connector_id.eq_any(merchant_connector_id));
        }

        if let Some(filter_currency) = &refund_list_details.currency {
            filter = filter.filter(dsl::currency.eq_any(filter_currency.clone()));
        }

        if let Some(filter_refund_status) = &refund_list_details.refund_status {
            filter = filter.filter(dsl::refund_status.eq_any(filter_refund_status.clone()));
        }

        logger::debug!(query = %diesel::debug_query::<diesel::pg::Pg, _>(&filter).to_string());

        db_metrics::track_database_call::<<Self as HasTable>::Table, _, _>(
            filter.get_results_async(conn),
            db_metrics::DatabaseOperation::Filter,
        )
        .await
        .change_context(errors::DatabaseError::NotFound)
        .attach_printable_lazy(|| "Error filtering records by predicate")
    }

    #[cfg(feature = "v2")]
    async fn filter_by_constraints(
        conn: &PgPooledConn,
        merchant_id: &common_utils::id_type::MerchantId,
        refund_list_details: refunds::RefundListConstraints,
        limit: i64,
        offset: i64,
    ) -> CustomResult<Vec<Self>, errors::DatabaseError> {
        let mut filter = <Self as HasTable>::table()
            .filter(dsl::merchant_id.eq(merchant_id.to_owned()))
            .order(dsl::modified_at.desc())
            .into_boxed();

        if let Some(payment_id) = &refund_list_details.payment_id {
            filter = filter.filter(dsl::payment_id.eq(payment_id.to_owned()));
        }

        if let Some(refund_id) = &refund_list_details.refund_id {
            filter = filter.filter(dsl::id.eq(refund_id.to_owned()));
        }

        if let Some(time_range) = &refund_list_details.time_range {
            filter = filter.filter(dsl::created_at.ge(time_range.start_time));

            if let Some(end_time) = time_range.end_time {
                filter = filter.filter(dsl::created_at.le(end_time));
            }
        }

        filter = match refund_list_details.amount_filter {
            Some(AmountFilter {
                start_amount: Some(start),
                end_amount: Some(end),
            }) => filter.filter(dsl::refund_amount.between(start, end)),
            Some(AmountFilter {
                start_amount: Some(start),
                end_amount: None,
            }) => filter.filter(dsl::refund_amount.ge(start)),
            Some(AmountFilter {
                start_amount: None,
                end_amount: Some(end),
            }) => filter.filter(dsl::refund_amount.le(end)),
            _ => filter,
        };

        if let Some(connector) = refund_list_details.connector {
            filter = filter.filter(dsl::connector.eq_any(connector));
        }

        if let Some(connector_id_list) = refund_list_details.connector_id_list {
            filter = filter.filter(dsl::connector_id.eq_any(connector_id_list));
        }

        if let Some(filter_currency) = refund_list_details.currency {
            filter = filter.filter(dsl::currency.eq_any(filter_currency));
        }

        if let Some(filter_refund_status) = refund_list_details.refund_status {
            filter = filter.filter(dsl::refund_status.eq_any(filter_refund_status));
        }

        filter = filter.limit(limit).offset(offset);

        logger::debug!(query = %diesel::debug_query::<diesel::pg::Pg, _>(&filter).to_string());

        db_metrics::track_database_call::<<Self as HasTable>::Table, _, _>(
            filter.get_results_async(conn),
            db_metrics::DatabaseOperation::Filter,
        )
        .await
        .change_context(errors::DatabaseError::NotFound)
        .attach_printable_lazy(|| "Error filtering records by predicate")

        // todo!()
    }

    #[cfg(feature = "v1")]
    async fn filter_by_meta_constraints(
        conn: &PgPooledConn,
        merchant_id: &common_utils::id_type::MerchantId,
        refund_list_details: &common_utils::types::TimeRange,
    ) -> CustomResult<api_models::refunds::RefundListMetaData, errors::DatabaseError> {
        let start_time = refund_list_details.start_time;

        let end_time = refund_list_details
            .end_time
            .unwrap_or_else(common_utils::date_time::now);

        let filter = <Self as HasTable>::table()
            .filter(dsl::merchant_id.eq(merchant_id.to_owned()))
            .order(dsl::modified_at.desc())
            .filter(dsl::created_at.ge(start_time))
            .filter(dsl::created_at.le(end_time));

        let filter_connector: Vec<String> = filter
            .clone()
            .select(dsl::connector)
            .distinct()
            .order_by(dsl::connector.asc())
            .get_results_async(conn)
            .await
            .change_context(errors::DatabaseError::Others)
            .attach_printable("Error filtering records by connector")?;

        let filter_currency: Vec<Currency> = filter
            .clone()
            .select(dsl::currency)
            .distinct()
            .order_by(dsl::currency.asc())
            .get_results_async(conn)
            .await
            .change_context(errors::DatabaseError::Others)
            .attach_printable("Error filtering records by currency")?;

        let filter_status: Vec<RefundStatus> = filter
            .select(dsl::refund_status)
            .distinct()
            .order_by(dsl::refund_status.asc())
            .get_results_async(conn)
            .await
            .change_context(errors::DatabaseError::Others)
            .attach_printable("Error filtering records by refund status")?;

        let meta = api_models::refunds::RefundListMetaData {
            connector: filter_connector,
            currency: filter_currency,
            refund_status: filter_status,
        };

        Ok(meta)
    }

    #[cfg(feature = "v1")]
    async fn get_refunds_count(
        conn: &PgPooledConn,
        merchant_id: &common_utils::id_type::MerchantId,
        refund_list_details: &refunds::RefundListConstraints,
    ) -> CustomResult<i64, errors::DatabaseError> {
        let mut filter = <Self as HasTable>::table()
            .count()
            .filter(dsl::merchant_id.eq(merchant_id.to_owned()))
            .into_boxed();

        let mut search_by_pay_or_ref_id = false;

        if let (Some(pid), Some(ref_id)) = (
            &refund_list_details.payment_id,
            &refund_list_details.refund_id,
        ) {
            search_by_pay_or_ref_id = true;
            filter = filter.filter(
                dsl::payment_id
                    .eq(pid.to_owned())
                    .or(dsl::refund_id.eq(ref_id.to_owned())),
            );
        };

        if !search_by_pay_or_ref_id {
            if let Some(pay_id) = &refund_list_details.payment_id {
                filter = filter.filter(dsl::payment_id.eq(pay_id.to_owned()));
            }
        }

        if !search_by_pay_or_ref_id {
            if let Some(ref_id) = &refund_list_details.refund_id {
                filter = filter.filter(dsl::refund_id.eq(ref_id.to_owned()));
            }
        }
        if let Some(profile_id) = &refund_list_details.profile_id {
            filter = filter.filter(dsl::profile_id.eq_any(profile_id.to_owned()));
        }

        if let Some(time_range) = refund_list_details.time_range {
            filter = filter.filter(dsl::created_at.ge(time_range.start_time));

            if let Some(end_time) = time_range.end_time {
                filter = filter.filter(dsl::created_at.le(end_time));
            }
        }

        filter = match refund_list_details.amount_filter {
            Some(AmountFilter {
                start_amount: Some(start),
                end_amount: Some(end),
            }) => filter.filter(dsl::refund_amount.between(start, end)),
            Some(AmountFilter {
                start_amount: Some(start),
                end_amount: None,
            }) => filter.filter(dsl::refund_amount.ge(start)),
            Some(AmountFilter {
                start_amount: None,
                end_amount: Some(end),
            }) => filter.filter(dsl::refund_amount.le(end)),
            _ => filter,
        };

        if let Some(connector) = refund_list_details.connector.clone() {
            filter = filter.filter(dsl::connector.eq_any(connector));
        }

        if let Some(merchant_connector_id) = refund_list_details.merchant_connector_id.clone() {
            filter = filter.filter(dsl::merchant_connector_id.eq_any(merchant_connector_id))
        }

        if let Some(filter_currency) = &refund_list_details.currency {
            filter = filter.filter(dsl::currency.eq_any(filter_currency.clone()));
        }

        if let Some(filter_refund_status) = &refund_list_details.refund_status {
            filter = filter.filter(dsl::refund_status.eq_any(filter_refund_status.clone()));
        }

        logger::debug!(query = %diesel::debug_query::<diesel::pg::Pg, _>(&filter).to_string());

        filter
            .get_result_async::<i64>(conn)
            .await
            .change_context(errors::DatabaseError::NotFound)
            .attach_printable_lazy(|| "Error filtering count of refunds")
    }

    #[cfg(feature = "v2")]
    async fn get_refunds_count(
        conn: &PgPooledConn,
        merchant_id: &common_utils::id_type::MerchantId,
        refund_list_details: refunds::RefundListConstraints,
    ) -> CustomResult<i64, errors::DatabaseError> {
        let mut filter = <Self as HasTable>::table()
            .count()
            .filter(dsl::merchant_id.eq(merchant_id.to_owned()))
            .into_boxed();

        if let Some(payment_id) = &refund_list_details.payment_id {
            filter = filter.filter(dsl::payment_id.eq(payment_id.to_owned()));
        }

        if let Some(refund_id) = &refund_list_details.refund_id {
            filter = filter.filter(dsl::id.eq(refund_id.to_owned()));
        }

        if let Some(time_range) = refund_list_details.time_range {
            filter = filter.filter(dsl::created_at.ge(time_range.start_time));

            if let Some(end_time) = time_range.end_time {
                filter = filter.filter(dsl::created_at.le(end_time));
            }
        }

        filter = match refund_list_details.amount_filter {
            Some(AmountFilter {
                start_amount: Some(start),
                end_amount: Some(end),
            }) => filter.filter(dsl::refund_amount.between(start, end)),
            Some(AmountFilter {
                start_amount: Some(start),
                end_amount: None,
            }) => filter.filter(dsl::refund_amount.ge(start)),
            Some(AmountFilter {
                start_amount: None,
                end_amount: Some(end),
            }) => filter.filter(dsl::refund_amount.le(end)),
            _ => filter,
        };

        if let Some(connector) = refund_list_details.connector {
            filter = filter.filter(dsl::connector.eq_any(connector));
        }

        if let Some(connector_id_list) = refund_list_details.connector_id_list {
            filter = filter.filter(dsl::connector_id.eq_any(connector_id_list));
        }

        if let Some(filter_currency) = refund_list_details.currency {
            filter = filter.filter(dsl::currency.eq_any(filter_currency));
        }

        if let Some(filter_refund_status) = refund_list_details.refund_status {
            filter = filter.filter(dsl::refund_status.eq_any(filter_refund_status));
        }

        logger::debug!(query = %diesel::debug_query::<diesel::pg::Pg, _>(&filter).to_string());

        filter
            .get_result_async::<i64>(conn)
            .await
            .change_context(errors::DatabaseError::NotFound)
            .attach_printable_lazy(|| "Error filtering count of refunds")
    }

    #[cfg(feature = "v1")]
    async fn get_refund_status_with_count(
        conn: &PgPooledConn,
        merchant_id: &common_utils::id_type::MerchantId,
        profile_id_list: Option<Vec<common_utils::id_type::ProfileId>>,
        time_range: &common_utils::types::TimeRange,
    ) -> CustomResult<Vec<(RefundStatus, i64)>, errors::DatabaseError> {
        let mut query = <Self as HasTable>::table()
            .group_by(dsl::refund_status)
            .select((dsl::refund_status, diesel::dsl::count_star()))
            .filter(dsl::merchant_id.eq(merchant_id.to_owned()))
            .into_boxed();

        if let Some(profile_id) = profile_id_list {
            query = query.filter(dsl::profile_id.eq_any(profile_id));
        }

        query = query.filter(dsl::created_at.ge(time_range.start_time));

        query = match time_range.end_time {
            Some(ending_at) => query.filter(dsl::created_at.le(ending_at)),
            None => query,
        };

        logger::debug!(filter = %diesel::debug_query::<diesel::pg::Pg,_>(&query).to_string());

        db_metrics::track_database_call::<<Self as HasTable>::Table, _, _>(
            query.get_results_async::<(RefundStatus, i64)>(conn),
            db_metrics::DatabaseOperation::Count,
        )
        .await
        .change_context(errors::DatabaseError::NotFound)
        .attach_printable_lazy(|| "Error filtering status count of refunds")
    }
}
