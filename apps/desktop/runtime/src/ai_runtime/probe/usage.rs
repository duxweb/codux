use serde::Deserialize;

#[derive(Default, Deserialize)]
pub(super) struct UsageTotalsFields {
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    cache_read_input_tokens: Option<i64>,
    reasoning_output_tokens: Option<i64>,
    total_tokens: Option<i64>,
}

#[derive(Clone, Default)]
pub(super) struct UsageTotals {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub total_tokens: i64,
}

pub(super) fn usage_totals_from_fields(fields: &UsageTotalsFields) -> Option<UsageTotals> {
    let raw_input_tokens = fields.input_tokens.unwrap_or(0);
    let raw_output_tokens = fields.output_tokens.unwrap_or(0);
    let cached_input_tokens = fields
        .cached_input_tokens
        .or(fields.cache_read_input_tokens)
        .unwrap_or(0);
    let reasoning_output_tokens = fields.reasoning_output_tokens.unwrap_or(0);
    if raw_input_tokens == 0 && raw_output_tokens == 0 {
        if let Some(raw_total_tokens) = fields.total_tokens {
            if raw_total_tokens > 0 {
                return Some(UsageTotals {
                    input_tokens: raw_total_tokens,
                    output_tokens: 0,
                    cached_input_tokens,
                    total_tokens: raw_total_tokens,
                });
            }
        }
    }
    let input_tokens = (raw_input_tokens - cached_input_tokens).max(0);
    let output_tokens = (raw_output_tokens - reasoning_output_tokens).max(0);
    let total_tokens = input_tokens + output_tokens + reasoning_output_tokens;
    if input_tokens == 0 && output_tokens == 0 && cached_input_tokens == 0 && total_tokens == 0 {
        return None;
    }
    Some(UsageTotals {
        input_tokens,
        output_tokens,
        cached_input_tokens,
        total_tokens,
    })
}

pub(super) fn resolve_runtime_usage(
    total_usage: Option<UsageTotals>,
    base_usage: Option<UsageTotals>,
    last_usage: Option<UsageTotals>,
) -> Option<UsageTotals> {
    if total_usage.is_none() && last_usage.is_none() {
        return None;
    }
    let Some(last_usage) = last_usage else {
        return total_usage;
    };
    let base_usage = base_usage.unwrap_or_default();
    if let Some(total_usage) = total_usage {
        let total_with_cache = total_usage.total_tokens + total_usage.cached_input_tokens;
        let base_with_cache = base_usage.total_tokens + base_usage.cached_input_tokens;
        if total_with_cache > base_with_cache {
            return Some(total_usage);
        }
        if total_with_cache == base_with_cache {
            let last_with_cache = last_usage.total_tokens + last_usage.cached_input_tokens;
            if last_with_cache == total_with_cache {
                return Some(total_usage);
            }
        }
    }
    Some(UsageTotals {
        input_tokens: base_usage.input_tokens + last_usage.input_tokens,
        output_tokens: base_usage.output_tokens + last_usage.output_tokens,
        cached_input_tokens: base_usage.cached_input_tokens + last_usage.cached_input_tokens,
        total_tokens: base_usage.total_tokens + last_usage.total_tokens,
    })
}
