extern crate core;

use crate::ast::*;
use crate::derivation_tree::DerivationTree;
use crate::identifiers::{Identifier, IdentifierServer};
pub use crate::parser::Parser;
use itertools::Itertools;
use logos_nom_bridge::Tokens;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

mod ast;
mod derivation_tree;
mod identifiers;
mod lexer;
mod parser;
mod unify;
mod union_find;

/// Sniffer's job is to saturate a set of rules, by deriving the current set until no
/// new rule can be added
#[derive(Default)]
pub struct Sniffer {
    rules: HashSet<InnerRule>,
    derived_from: HashMap<InnerRule, Vec<InnerRule>>,

    id_server: IdentifierServer,
}
impl Sniffer {
    /// Creates a Sniffer context from a `.pif` file
    pub fn new<P: AsRef<Path>>(file: P) -> Result<Sniffer, ()> {
        // Parses the `.pif` file
        let mut file_contents = String::new();
        if let Ok(mut file) = File::open(file) {
            file.read_to_string(&mut file_contents).unwrap();
        } else {
            return Err(())
        }
        let parsed_rules =
            Parser::parse_rules(Tokens::new(&file_contents)).expect("failed to parse file");

        // Then maps every string id to an inner identifier
        let mut sniffer = Sniffer::default();
        for rule in parsed_rules {
            if rule.premises.is_empty() {
                let inner_axiom = Atom::from((&rule.conclusion, &mut sniffer.id_server));
                sniffer.add_axiom(inner_axiom);
            } else {
                sniffer
                    .clauses
                    .insert(Rule::from((&rule, &mut sniffer.id_server)));
            }
        }
        Ok(sniffer)
    }

    /// Returns a derivation that results in a given rule if one exists
    pub fn find(&mut self, atom: &Atom<String>) -> Result<DerivationTree, SaturationFailure> {
        let inner_atom = Atom::from((atom, &mut self.id_server));

        // We keep saturating our rule set until we either find our atom or the set is fully saturated
        while !self.axioms.contains(&inner_atom) {
            self.saturate()?
        }

        Ok(self.derivation_tree(atom).unwrap())
    }

    fn saturate(&mut self) -> Result<(), SaturationFailure> {
        // We derive new rules through resolution:
        // `p`, `p => q` |= `q`
        // In order to do so, we try to unify each and every axiom to every rule's premisses, until
        // one matches. When this happens, the conclusion can be added to the set of axioms
        // TODO: use a more clever selection function in order to avoid exponential/infinite growth
        let mut derived = vec![];
        for rule in &self.generative_rules {
            for input in self
                .axioms
                .iter()
                .cloned()
                .combinations(rule.premises.len())
            {
                if let Ok(resulting_rule) = rule.assign(input.as_slice()) {
                    derived.push(resulting_rule);
                }
            }
        }

        let mut modified = false;
        for r in derived {
            modified |= self.add_derived_axiom(r.conclusion, r.premises);
        }

        // Check if there are any new axioms that aren't already registered
        if modified {
            Ok(())
        } else {
            Err(SaturationFailure::Saturated)
        }
    }

    /// Adds a new derived axiom, return `false` if it was already present
    pub fn add_derived_axiom(&mut self, axiom: InnerAtom, derived_from: Vec<InnerAtom>) -> bool {
        self.derived_from
            .entry(axiom.clone())
            .or_insert_with(|| derived_from);
        self.axioms.insert(axiom)
    }

    /// Adds a new axiom, returning `false` if it was already present
    /*
    pub fn add_axiom(&mut self, axiom: InnerAtom) -> bool {
        self.derived_from.insert(axiom.clone(), vec![]);
        self.axioms.insert(axiom)
    }
    */

    pub fn add_inner_rule(&mut self, rule: InnerRule) -> bool {
        self.clauses.insert(rule);
        self.derived_from.insert(axiom.clone(), vec![]);
    }

    /// Returns the derivation tree for a given axiom
    pub fn derivation_tree(&self, root: &Atom<String>) -> Option<DerivationTree> {
        fn inner(root: &InnerAtom, sniffer: &Sniffer) -> Option<DerivationTree> {
            let mut decision_tree =
                DerivationTree::new(Atom::try_from((root, &sniffer.id_server)).ok()?);
            let premises = sniffer.derived_from.get(root)?;
            for pre in premises {
                decision_tree.insert(inner(pre, sniffer)?)
            }
            Some(decision_tree)
        }

        let inner_atom = Atom::try_from((root, &self.id_server)).ok()?;
        let premises = self.derived_from.get(&inner_atom)?;

        let mut decision_tree = DerivationTree::new(root.clone());
        for pre in premises {
            decision_tree.insert(inner(pre, self)?)
        }
        Some(decision_tree)
    }

    pub fn rules_to_string(&self) -> String {
        let mut rules = String::new();
        for rule in &self.generative_rules {
            let rule = Rule::try_from((rule, &self.id_server)).unwrap();
            rules.push_str(&format!("\t{}\n", rule));
        }
        rules
    }

    pub fn axioms_to_string(&self) -> String {
        let mut axioms = String::new();
        for axiom in &self.axioms {
            let axiom = Atom::try_from((axiom, &self.id_server)).unwrap();
            axioms.push_str(&format!("\t{}\n", axiom));
        }
        axioms
    }

    pub fn derived_from_to_string(&self) -> String {
        let mut derived_from = String::new();
        for (axiom, premises) in &self.derived_from {
            let axiom = Atom::try_from((axiom, &self.id_server)).unwrap();
            derived_from.push_str(&format!("\t{}:\n", axiom));

            // draw derivation tree

            for p in premises {
                let p = Atom::try_from((p, &self.id_server)).unwrap();
                // ptree::print_tree(self.derivation_tree(&p));
                derived_from.push_str(&format!("\t\t{}\n", p));
            }
        }
        derived_from
    }

    pub fn print_derived_from(&self) {
        for (axiom, _) in &self.derived_from{
            let atome = Atom::try_from((axiom, &self.id_server)).unwrap();
            ptree::print_tree(&self.derivation_tree(&atome).unwrap()).unwrap();
        }
    }
}

impl std::fmt::Display for Sniffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Rules:")?;
        write!(f, "{}", self.rules_to_string())?;

        writeln!(f, "Axioms: ")?;
        write!(f, "{}", self.axioms_to_string())?;

        writeln!(f, "Derived from: ")?;
        write!(f, "{}", self.derived_from_to_string())?;
        Ok(())
    }
}

/// Represents the result of a saturation attempt
pub enum SaturationFailure {
    Saturated,     // The saturation attempt did not create any new rule
    DerivedBottom, // The saturation derived a contradiction
}
