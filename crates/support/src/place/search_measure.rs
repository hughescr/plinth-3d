//! Two batched admission probes: heads first, then columns for surviving heads.
use super::*;
pub(super) fn probe(
    cache: &mut Cache,
    builder: &PlanBuilder<Contacts>,
    alternatives: Vec<ContactCandidate>,
    sites: Vec<NeedPoint>,
    loads: Vec<f64>,
    obstacles: Option<&[Chosen]>,
) -> usize {
    let mut answers = Vec::new();
    builder.probe_alternatives(&alternatives, &mut answers);
    let count = answers.iter().filter(|q| q.is_ok()).count();
    // Optional spread search has an immutable baseline. A certified head that
    // already collides with it cannot be selected, regardless of its column.
    // Reject those in this batch before spending a model-clearance column probe.
    // Replacements and mandatory placement do not use this exclusion.
    let fits: Vec<_> = answers
        .iter()
        .map(|q| {
            q.as_ref().is_ok_and(|q| {
                obstacles.is_none_or(|old| old.iter().all(|c| q.compatible(&c.quote)))
            })
        })
        .collect();
    let valid: Vec<_> = answers
        .iter()
        .zip(&fits)
        .filter(|(_, fits)| **fits)
        .map(|(q, _)| q.as_ref().expect("certified head"))
        .collect();
    let column_loads: Vec<_> = loads
        .iter()
        .zip(&fits)
        .filter_map(|(&load, &fits)| fits.then_some(load))
        .collect();
    let mut columns = builder
        .probe_column_alternatives(&valid, &column_loads)
        .into_iter();
    for (((candidate, site), answer), fits) in
        alternatives.into_iter().zip(sites).zip(answers).zip(fits)
    {
        if answer.is_ok() && !fits {
            // A later baseline may move an old head during mandatory growth.
            // This layout-local rejection must not poison the immutable-model
            // probe cache for that later layout.
            cache.seen.remove(&geometry_key(&candidate));
            *cache
                .rejected
                .entry("head pair collision before column probe".into())
                .or_default() += 1;
            continue;
        }
        let answer = answer.and_then(|quote| {
            columns
                .next()
                .expect("one column per valid quote")
                .map(|column| (quote, column))
        });
        match answer {
            Ok((quote, column)) => cache.available.push(Chosen {
                candidate,
                quote,
                column,
                site,
                patch_history: Vec::new(),
            }),
            Err(e) => {
                *cache
                    .rejected
                    .entry(
                        format!("{e:?}")
                            .split(" {")
                            .next()
                            .unwrap_or("unknown")
                            .to_owned(),
                    )
                    .or_default() += 1
            }
        }
    }
    count
}
