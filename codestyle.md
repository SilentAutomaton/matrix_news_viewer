# Code style

One standard for every tier. The tier decides only where this file is written
down, never what it says.

| Rules | Apply |
|---|---|
| Shape, naming, comments, the "do not" list | always, a one-file script included |
| Modularity and boundaries | when the code has more than one module |
| Errors and text the user reads | when a person reads the program's output |
| Borrowed code headers | when code is copied from elsewhere |

One rule above the others: **the code must read cleanly with every comment
deleted.** Comments explain why, never what.

## Shape

- Functions stay short. If one needs a section header comment, it wanted to be
  two functions.
- Type hints everywhere. No docstrings.
- Explicit imports. No `from x import *`, no re-export shims.
- Prefer the standard library, then what is already installed. A new dependency
  must beat a few lines of your own code.

## Do not

- No metaclasses, no descriptors, no `__getattr__` tricks.
- No inheritance deeper than one level, and no abstract base class for a
  protocol with one implementation.
- No factories, builders or registries beyond the ones the project already has.
- No `**kwargs` forwarded through layers. Name the parameters.
- No error handling for cases that cannot happen. Trust internal guarantees.
- No compatibility shims for code that was removed.

## Naming

Say what the thing is, not what it is made of.

```python
# no
def proc(d, f=None): ...
mgr = ModuleManagerFactory().create()

# yes
def draw_rite(config: Config) -> Rite: ...
```

## Comments

Only where the logic is not obvious from the code. English only. No section
banners, no commented out code, no notes about tools, plugins, models or
assistants.

```python
# no
# loop over modules
for module in modules:

# yes
# Modules drawn recently are penalised, not banned: a repeat must stay possible
# or a coincidence would mean nothing.
weight = base / (penalty if name in recent else 1)
```

## Modularity

Applies once the code has more than one module. A single script skips this
section whole.

- A structure, not a dictionary, crosses a module boundary. Dataclasses in
  Python, the equivalent record type elsewhere.
- The contracts live in one place, and every module imports them from there.
- A module never reaches into another module's internals.
- A module can be deleted without editing the others.
- A new module copies the shape of the modules already there. One file, one
  entry point.

## Errors the user sees

Applies when a person reads the output. Three parts, in this order: what
happened, why, what to do. No exception class names, no tracebacks, and nothing
that blames the reader.

```python
# no
raise RuntimeError("SDR init failed (errno 19)")

# yes
raise Unavailable(
    "No radio found. The RTL-SDR dongle is not plugged in. "
    "Connect it and run :checkhealth."
)
```

User visible text lives in one module, never inline.

## Borrowed code

Keep the original header. Author, link, licence, and one line on what was taken.

```python
# Von Neumann debiasing and SHA-512 whitening.
# Adapted from rtl-entropy by Paul Warren, GPL-3.0.
# https://github.com/pwarren/rtl-entropy
```

If the licence does not allow the copy, reimplement from the description and say
so: `# Method described in <link>; implementation is our own.`
