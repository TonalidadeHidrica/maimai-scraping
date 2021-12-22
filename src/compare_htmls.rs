use anyhow::bail;
use ego_tree::iter::Edge;
use itertools::{EitherOrBoth, Itertools};
use scraper::{ElementRef, Node};
use typed_html::types::{Class, SpacedSet};

pub fn elements_are_equivalent(
    reconstructed: ElementRef,
    actual: ElementRef,
) -> anyhow::Result<()> {
    let mut reconstructed = to_show(reconstructed);
    let mut actual = to_show(actual);
    for pair in reconstructed.by_ref().zip_longest(actual.by_ref()) {
        let (cons, actu) = match pair {
            EitherOrBoth::Both(a, b) => (a, b),
            EitherOrBoth::Left(a) => {
                bail!("Too many elements found: {:?}", a);
            }
            EitherOrBoth::Right(b) => {
                bail!("More elements expected: {:?}", b);
            }
        };
        if cons.0 != actu.0 {
            bail!("Found1 {:?}, expected {:?}", cons, actu);
        }
        same(cons.1, actu.1)?
    }
    Ok(())
}

fn to_show(element: ElementRef) -> impl Iterator<Item = (bool, &Node)> + '_ {
    element
        .traverse()
        .map(|x| match x {
            Edge::Open(e) => (true, e),
            Edge::Close(e) => (false, e),
        })
        .map(|(x, y)| (x, y.value()))
        .filter(|x| x.1.as_text().map_or(true, |x| !x.trim().is_empty()))
}

fn same(a: &Node, b: &Node) -> anyhow::Result<()> {
    if let (Some(a), Some(b)) = (a.as_element(), b.as_element()) {
        if a.name() != b.name() {
            bail!(
                "Tag names are different: found {:?}, expected {:?}",
                a.name(),
                b.name()
            );
        }
        let (mut a, mut b) = (a.attrs().sorted(), b.attrs().sorted());
        for ab in (&mut a).zip_longest(&mut b) {
            let (a, b) = match ab {
                EitherOrBoth::Both(a, b) => (a, b),
                EitherOrBoth::Left(a) => {
                    bail!("Too many attributes found: {:?}", a);
                }
                EitherOrBoth::Right(b) => {
                    bail!("More attributes expected: {:?}", b);
                }
            };
            let same = if a.0 == "class" && b.0 == "class" {
                SpacedSet::<Class>::try_from(a.1) == SpacedSet::<Class>::try_from(b.1)
            } else if [a.0, b.0]
                .iter()
                .all(|x| ["accept-charset", "accept_charset"].iter().any(|y| x == y))
            {
                a.1 == b.1
            } else {
                a == b
            };
            if !same {
                bail!(
                    "These attributes are not same: found {:?}, expected {:?}",
                    a,
                    b
                );
            }
        }
        Ok(())
    } else if let (Some(a), Some(b)) = (a.as_text(), b.as_text()) {
        if a.trim() == b.trim() {
            Ok(())
        } else {
            bail!("These text are different: found {:?}, expected {:?}", a, b);
        }
    } else if a == b {
        Ok(())
    } else {
        bail!(
            "These elements are not same: found {:?}, expected {:?}",
            a,
            b
        )
    }
}
