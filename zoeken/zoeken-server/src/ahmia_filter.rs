//! Drop `.onion` results present in Ahmia's blacklist, when routing through Tor.

use md5::{Digest as _, Md5};
use zoeken_data::AhmiaBlacklist;
use zoeken_results::Result_;
use zoeken_search::ResultContainer;

fn md5_hex(value: &str) -> String {
    hex::encode(Md5::digest(value.as_bytes()))
}

fn onion_host(url: &str) -> Option<String> {
    let host = url::Url::parse(url).ok()?.host_str()?.to_ascii_lowercase();
    host.ends_with(".onion").then_some(host)
}

fn url_of(result: &Result_) -> Option<&str> {
    match result {
        Result_::Main(r) => Some(&r.url),
        Result_::Image(r) => Some(&r.url),
        Result_::Paper(r) => Some(&r.url),
        Result_::Code(r) => Some(&r.url),
        Result_::File(r) => Some(&r.url),
        Result_::KeyValue(r) => Some(&r.url),
        _ => None,
    }
}

pub(crate) fn filter_blacklisted_onions(
    container: &mut ResultContainer,
    blacklist: &AhmiaBlacklist,
    using_tor_proxy: bool,
) {
    if !using_tor_proxy || blacklist.is_empty() {
        return;
    }
    container.results.retain(|result| {
        let Some(host) = url_of(result).and_then(onion_host) else {
            return true;
        };
        !blacklist.contains(&md5_hex(&host))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoeken_results::MainResult;

    fn container(url: &str) -> ResultContainer {
        ResultContainer {
            results: vec![Result_::Main(MainResult {
                url: url.to_string(),
                ..MainResult::default()
            })],
            ..ResultContainer::default()
        }
    }

    #[test]
    fn drops_blacklisted_onion_result_when_using_tor() {
        let mut blacklist = AhmiaBlacklist::default();
        blacklist.insert(md5_hex("blockedxxxxxxxxx.onion"));
        let mut c = container("http://blockedxxxxxxxxx.onion/page");
        filter_blacklisted_onions(&mut c, &blacklist, true);
        assert!(c.results.is_empty());
    }

    #[test]
    fn keeps_non_blacklisted_onion_result() {
        let mut blacklist = AhmiaBlacklist::default();
        blacklist.insert(md5_hex("other.onion"));
        let mut c = container("http://allowedxxxxxxxx.onion/page");
        filter_blacklisted_onions(&mut c, &blacklist, true);
        assert_eq!(c.results.len(), 1);
    }

    #[test]
    fn does_nothing_when_not_using_tor() {
        let mut blacklist = AhmiaBlacklist::default();
        blacklist.insert(md5_hex("blockedxxxxxxxxx.onion"));
        let mut c = container("http://blockedxxxxxxxxx.onion/page");
        filter_blacklisted_onions(&mut c, &blacklist, false);
        assert_eq!(c.results.len(), 1);
    }

    #[test]
    fn leaves_non_onion_results_alone() {
        let blacklist = AhmiaBlacklist::default();
        let mut c = container("https://example.com/page");
        filter_blacklisted_onions(&mut c, &blacklist, true);
        assert_eq!(c.results.len(), 1);
    }
}
