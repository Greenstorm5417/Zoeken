use std::cell::Cell;

use proptest::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttemptOutcome {
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RetryResult {
    succeeded: bool,
    attempts_made: u32,
}

fn run_retry_loop<F>(retries: u32, mut transport: F) -> RetryResult
where
    F: FnMut(u32) -> AttemptOutcome,
{
    let max_attempts = retries.saturating_add(1);
    let mut attempt: u32 = 0;

    loop {
        attempt += 1;
        let outcome = transport(attempt);
        let last_attempt = attempt >= max_attempts;

        match outcome {
            AttemptOutcome::Success => {
                return RetryResult {
                    succeeded: true,
                    attempts_made: attempt,
                };
            }
            AttemptOutcome::Failure => {
                if !last_attempt {
                    continue;
                }
                return RetryResult {
                    succeeded: false,
                    attempts_made: attempt,
                };
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn retry_count_is_honored(retries in 0u32..=16, fail_for in 0u32..=24) {
        let max_attempts = retries + 1;

        let calls = Cell::new(0u32);
        let transport = |attempt: u32| {
            calls.set(calls.get() + 1);
            prop_assert_eq_local(attempt, calls.get());
            if attempt <= fail_for {
                AttemptOutcome::Failure
            } else {
                AttemptOutcome::Success
            }
        };

        let result = run_retry_loop(retries, transport);

        prop_assert!(
            result.attempts_made <= max_attempts,
            "attempts {} exceeded budget r+1 = {}",
            result.attempts_made,
            max_attempts
        );

        prop_assert_eq!(calls.get(), result.attempts_made);

        if retries == 0 {
            prop_assert_eq!(result.attempts_made, 1);
            prop_assert_eq!(result.succeeded, fail_for == 0);
        }

        if fail_for < max_attempts {
            prop_assert!(result.succeeded);
            prop_assert_eq!(result.attempts_made, fail_for + 1);
        } else {
            prop_assert!(!result.succeeded);
            prop_assert_eq!(result.attempts_made, max_attempts);
        }
    }
}

fn prop_assert_eq_local(actual: u32, expected: u32) {
    assert_eq!(
        actual, expected,
        "transport invoked with non-contiguous attempt number"
    );
}
