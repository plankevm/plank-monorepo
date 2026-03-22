```
Report = &[Group]

Group
├── primary_level: Level
├── title: Option<Title>               # carries level
└── elements: Vec<Element>

Element
├── Message(Message)                   # level + str
├── Cause(Snippet<Annotation>)
├── Suggestion(Snippet<Patch>)
├── Origin(Origin)
└── Padding(Padding)

Snippet<T>                             # T = Annotation | Patch
├── path: Option<Cow<str>>
├── source: Cow<str>
├── line_start: usize
├── markers: Vec<T>
└── fold: bool

Annotation
├── span: Range<usize>
├── label: Option<Cow<str>>
├── kind: AnnotationKind               # Primary | Context | Visible
└── highlight_source: bool

Patch
├── span: Range<usize>
└── replacement: Cow<str>

Origin
├── path: Cow<str>
├── line: Option<usize>
└── char_column: Option<usize>

```
