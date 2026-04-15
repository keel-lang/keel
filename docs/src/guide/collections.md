# Collections & Lambdas

## Lists

```keel
nums = [1, 2, 3, 4, 5]
names = ["alice", "bob", "charlie"]
empty = []
```

## Collection methods

All methods accept lambdas (`x => expr`) or task references.

### map — transform each element

```keel
doubled = [1, 2, 3].map(n => n * 2)         # [2, 4, 6]
names = contacts.map(c => c.name)            # extract names
```

### filter — keep matching elements

```keel
big = [1, 5, 10, 20].filter(n => n > 5)     # [10, 20]
urgent = emails.filter(e => e.urgency == high)
```

### find — first matching element

```keel
found = items.find(n => n > 15)              # first match or none
admin = users.find(u => u.role == "admin") ?? default_user
```

### any / all — boolean checks

```keel
has_urgent = emails.any(e => e.urgency == critical)    # true/false
all_done = tasks.all(t => t.status == "complete")
```

### sort_by — sort by derived key

```keel
sorted = items.sort_by(item => item.name)
by_age = people.sort_by(p => p.age)
```

### flat_map — map and flatten

```keel
all_tags = posts.flat_map(p => p.tags)       # flattens nested lists
```

## Lambda syntax

```keel
# Single parameter
n => n * 2

# Multi-parameter
(a, b) => a + b

# Block body
items.map(item => {
  urgency = triage(item)
  {item: item, urgency: urgency}
})
```

## Task references

Named tasks can be passed directly to collection methods:

```keel
task double(n: int) -> int { n * 2 }
task is_even(n: int) -> bool { n % 2 == 0 }

result = [1, 2, 3].map(double)        # [2, 4, 6]
evens = [1, 2, 3, 4].filter(is_even)  # [2, 4]
```

## Map / struct access

```keel
info = {name: "Alice", age: 30}
info.name                              # "Alice"
info.age                               # 30

# Nested
team = {lead: {name: "Bob", role: "eng"}}
team.lead.name                         # "Bob"
```

## String methods

```keel
s = "  Hello, World!  "
s.trim()                  # "Hello, World!"
s.upper()                 # "  HELLO, WORLD!  "
s.lower()                 # "  hello, world!  "
s.contains("Hello")       # true
s.starts_with("  H")      # true
s.split(", ")             # ["  Hello", "World!  "]
s.replace("World", "Keel") # "  Hello, Keel!  "
"42".to_int()             # 42 (int?)
```
