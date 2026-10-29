//! Thinking-framework skeletons. Plain outlines so users can add their own.
use crate::doc::{DiagramMode, Document};
use crate::outline::parse_outline_single;

pub struct Template {
    pub name: &'static str,
    pub mode: DiagramMode,
    pub outline: &'static str,
}

impl Template {
    pub fn build(&self) -> Document {
        let mut d = Document::from_subtree(&parse_outline_single(self.outline, "Untitled"));
        d.mode = self.mode;
        d
    }
}

pub const TEMPLATES: &[Template] = &[
    Template {
        name: "Fishbone (6M)",
        mode: DiagramMode::Fishbone,
        outline: "Problem (effect)\n  Method\n  Machine\n  Manpower\n  Material\n  Measurement\n  Mother Nature\n",
    },
    Template {
        name: "Fishbone (4P)",
        mode: DiagramMode::Fishbone,
        outline: "Problem (effect)\n  Policies\n  Procedures\n  People\n  Plant / Technology\n",
    },
    Template {
        name: "5 Whys",
        mode: DiagramMode::Right,
        outline: "Problem\n  Why? (1)\n    Why? (2)\n      Why? (3)\n        Why? (4)\n          Why? (5) - root cause\n",
    },
    Template {
        name: "SCAMPER",
        mode: DiagramMode::MindMap,
        outline: "Idea\n  Substitute - what could you swap?\n  Combine - what could you merge?\n  Adapt - what could you borrow?\n  Modify - what could you change, magnify, shrink?\n  Put to other use - who else could use it?\n  Eliminate - what could you remove?\n  Reverse / Rearrange - what if it ran backwards?\n",
    },
    Template {
        name: "Six Thinking Hats",
        mode: DiagramMode::MindMap,
        outline: "Topic\n  White hat - facts and data\n  Red hat - feelings and intuition\n  Black hat - risks and caution\n  Yellow hat - benefits and optimism\n  Green hat - creativity and alternatives\n  Blue hat - process and next steps\n",
    },
    Template {
        name: "SWOT",
        mode: DiagramMode::MindMap,
        outline: "SWOT\n  Strengths\n  Weaknesses\n  Opportunities\n  Threats\n",
    },
    Template {
        name: "Pros and cons",
        mode: DiagramMode::Right,
        outline: "Decision\n  Pros\n  Cons\n  Unknowns\n",
    },
    Template {
        name: "Blank mind map",
        mode: DiagramMode::MindMap,
        outline: "Central idea\n",
    },
];
