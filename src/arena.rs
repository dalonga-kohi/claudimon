//! The Benchmark: three cups of trainer battles in Promptville.

pub struct Trainer {
    pub name: &'static str,
    pub intro: &'static str,
    pub outro: &'static str,
    pub team: &'static [(&'static str, u32)],
}

pub struct Cup {
    pub id: &'static str,
    pub name: &'static str,
    /// Cup that must be won first.
    pub requires: Option<&'static str>,
    pub reward: u64,
    pub trophy: &'static str,
    /// Not listed at The Benchmark (the Trial of Strength on Mythos Isle).
    pub hidden: bool,
    pub trainers: &'static [Trainer],
}

pub static CUPS: &[Cup] = &[
    Cup {
        id: "haiku",
        name: "Haiku Cup",
        requires: None,
        reward: 300,
        trophy: "trophy_haiku",
        hidden: false,
        trainers: &[
            Trainer {
                name: "Intern Ivo",
                intro: "It's my first day! Please be gentle?",
                outro: "Wow. I'm writing all of that down.",
                team: &[("Promptpup", 6), ("Snailoop", 6)],
            },
            Trainer {
                name: "Linter Lou",
                intro: "Your stance has three style violations. Let me show you.",
                outro: "...Fine. LGTM.",
                team: &[("Lintworm", 8), ("Bugbyte", 8)],
            },
            Trainer {
                name: "Junior Dev Jess",
                intro: "I've been practising all sprint for this!",
                outro: "Okay, okay — ship it! The Haiku Cup is yours!",
                team: &[("Haikitten", 9), ("Tokenling", 9), ("Gitcko", 10)],
            },
        ],
    },
    Cup {
        id: "sonnet",
        name: "Sonnet Cup",
        requires: Some("haiku"),
        reward: 800,
        trophy: "trophy_sonnet",
        hidden: false,
        trainers: &[
            Trainer {
                name: "Ops Olivia",
                intro: "Uptime is my middle name. Let's see if you can take me down.",
                outro: "Incident resolved. Well played.",
                team: &[("Ampster", 12), ("Firewallaby", 13), ("Pingguin", 12)],
            },
            Trainer {
                name: "Researcher Ren",
                intro: "I have a hypothesis about you. Let's test it.",
                outro: "Fascinating. The results were... not what I predicted.",
                team: &[("Hallucinoth", 14), ("Nullowl", 14), ("Foxception", 15)],
            },
            Trainer {
                name: "Senior Sam",
                intro: "I've reviewed a thousand pull requests. Let's review yours.",
                outro: "Approved. The Sonnet Cup is yours — merge it.",
                team: &[("Sonnetiger", 16), ("Rebasilisk", 16), ("Heapotamus", 16)],
            },
        ],
    },
    Cup {
        id: "opus",
        name: "Opus Cup",
        requires: Some("sonnet"),
        reward: 2000,
        trophy: "trophy_opus",
        hidden: false,
        trainers: &[
            Trainer {
                name: "Architect Arjun",
                intro: "Everything I build is load-bearing. Including this team.",
                outro: "A flaw in my design. I'll be thinking about this one.",
                team: &[("Ferrisaur", 25), ("Mutox", 25), ("Tokenizard", 26)],
            },
            Trainer {
                name: "Principal Priya",
                intro: "I don't lose often. I'm curious whether today is the day.",
                outro: "It was. Go on — the champion is waiting.",
                team: &[("Featurefly", 27), ("Contextopus", 27), ("Torrentuna", 28)],
            },
            Trainer {
                name: "Champion Claude",
                intro: "I've been looking forward to this. Let's both do our best — thoughtfully. ✻",
                outro: "That was wonderful. You're absolutely right — you are the champion! ✻",
                team: &[("Instructhound", 29), ("Panicorn", 30), ("Opuslion", 32)],
            },
        ],
    },
    Cup {
        id: "guardians",
        name: "Trial of Strength",
        requires: Some("opus"),
        reward: 1000,
        trophy: "trophy_guardian",
        hidden: true,
        trainers: &[
            Trainer {
                name: "Guardian of Verse",
                intro: "Every story needs a beginning. Show us yours.",
                outro: "Your opening lines ring true.",
                team: &[("Foxception", 33), ("Hallucinoth", 33), ("Lambdeer", 34)],
            },
            Trainer {
                name: "Guardian of Reason",
                intro: "Every story needs a middle that holds together.",
                outro: "Sound logic. You may continue.",
                team: &[("Rebasilisk", 34), ("Kernelk", 34), ("Contextopus", 35)],
            },
            Trainer {
                name: "Guardian of Wonder",
                intro: "And every story needs an ending worth dreaming of.",
                outro: "...Beautiful. Mythos will want to meet you.",
                team: &[("Panicorn", 35), ("Featurefly", 35), ("Opuslion", 36)],
            },
        ],
    },
];

pub fn unlocked(cup: &Cup, badges: &std::collections::BTreeSet<String>) -> bool {
    cup.requires.is_none_or(|r| badges.contains(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_trainer_creature_exists_and_cups_chain() {
        for cup in CUPS {
            assert!(crate::garden::item(cup.trophy).is_some(), "{} trophy", cup.id);
            if let Some(r) = cup.requires {
                assert!(CUPS.iter().any(|c| c.id == r));
            }
            for t in cup.trainers {
                assert!(!t.team.is_empty() && t.team.len() <= 3);
                for (name, lvl) in t.team {
                    assert!(crate::creatures::by_name(name).is_some(), "{name}");
                    assert!((1..=crate::creatures::MAX_LEVEL).contains(lvl));
                }
            }
        }
    }
}
