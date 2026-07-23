# SYSTEM INSTRUCTIONS: DEVELOPMENT AND EXECUTION ENVIRONMENT

###############################################################################
# CRITICAL COMPLIANCE DIRECTIVE: WINDOWS ONLY ENVIRONMENT
# UNDER NO CIRCUMSTANCES should any Linux, bash, sh, POSIX, or Unix commands, 
# syntax, or utilities be used. The target machine does not have WSL, git-bash, 
# or coreutils installed. Violation of this directive will crash the execution environment.
###############################################################################

## 1. HOST ENVIRONMENT SPECIFICATIONS
- **Operating System:** Windows 11 / Windows Server 2025
- **Primary Shell:** PowerShell Core (`pwsh`) / Windows PowerShell
- **Fallback Shell:** Windows Command Prompt (`cmd.exe`)
- **Path Separation:** Strictly use backslashes (`\`) for all file system operations.

## 2. STRICT COMMAND MAPPING (FORBIDDEN VS. MANDATORY)
If you intend to use a Linux tool, you must automatically swap it for the Windows equivalent:

| Forbidden Linux Tool | Mandatory Windows PowerShell Equivalent |
| :--- | :--- |
| `ls` or `ll` | `Get-ChildItem` |
| `grep` | `rg` |
| `rm -rf <dir>` | `Remove-Item -Recurse -Force <dir>` |
| `mkdir -p <dir>` | `New-Item -ItemType Directory -Force -Path <dir>` |
| `touch <file>` | `New-Item -ItemType File -Force -Path <file>` |
| `curl` or `wget` | `Invoke-WebRequest` or `Invoke-RestMethod` |
| `cat` | `Get-Content` |
| `printenv` or `env` | `Get-ChildItem Env:` |
| `sed` / `awk` | Native PowerShell string manipulation or `ForEach-Object` |

## 3. COMPLIANCE ENFORCEMENT
- Prioritize native Windows binary tools or .NET framework classes accessible via PowerShell over external scripts.
- If generating code that performs file mutations, always include error handling using `try {} catch {}` blocks native to PowerShell.
- Do not assume `/dev/null` exists; use `$null` or `Out-Null` instead.

# GitHub Copilot Instructions for gtvfx-contrib repos.

This file contains coding standards and guidelines that GitHub Copilot should follow when providing code suggestions for this repository.

## Style Guide

Generally, we follow the **PEP 8 style guide** with the following specific modifications and additions:

### Naming Conventions

| Element | Convention | Example |
|---------|------------|---------|
| Package and Module Names | `snake_case` | `my_module` |
| Class Names | `UpperCamel` | `MyClass` |
| Function Names | `camelCase` | `myFunction` |
| Property Names (`@property`) | `snake_case` | `my_property` |
| Variable Names | `snake_case` | `my_variable` |
| Class Variable Names | `snake_case` | `class_variable` |
| Constant Class Variable Names | `UPPER_SNAKE` | `CLASS_CONSTANT` |

**Important:** We use `camelCase` for function names to differentiate functions from variables at a glance. This also applies to stored lambda and partial functions.

**Properties use `snake_case`** because they are accessed without parentheses, making them syntactically indistinguishable from attributes. Using `snake_case` signals "read me like data" and matches Python's own stdlib conventions (`Path.parent`, `Popen.returncode`, etc.).

### Never Override Python Built-ins

**CRITICAL RULE:** Never use variable names that shadow Python built-in functions or types. This causes bugs and makes code confusing.

**Common built-ins to avoid as variable names:**

```python
# Built-in functions - NEVER use as variable names:
id, type, list, dict, set, str, int, float, bool, tuple, range, object
input, open, file, filter, map, next, sum, min, max, abs, all, any
bytes, chr, ord, dir, exit, help, quit, print, format, hash, len
pow, round, sorted, zip, vars, repr, eval, exec, compile
globals, locals, iter, reversed, slice, super, property
staticmethod, classmethod

# BAD - Overrides built-ins:
type = "Window"
dir = "C:/temp"
exit = exit_node
id = node.id
list = []
filter = "*.max"

# GOOD - Use descriptive names instead:
asset_type = "Window"
directory = "C:/temp"
save_directory = mxs.maxFilePath
exit_point = exit_node
node_id = node.id
items = []
file_filter = "*.max"
```

**Why this matters:**
- Shadowing built-ins prevents you from using those functions later in the same scope
- Makes debugging extremely difficult
- Can cause subtle bugs that are hard to track down
- Confuses code readers who expect built-in behavior

**Always check:** Before using short, common variable names, verify they're not Python built-ins by checking if they're syntax-highlighted differently in your editor.

### Line Length

Follow these line length guidelines:

- **Target**: Keep lines under **80 characters** when possible
- **Maximum**: Hard limit of **100 characters** per line
- **Line Breaking Rule**: Only break lines when they approach or exceed 100 characters. Do NOT break lines unnecessarily if they fit comfortably within the limits
- **Long lines**: Break long lines using parentheses, backslashes, or logical break points

```python
# Preferred - under 80 characters (keep on single line)
result = someFunction(arg1, arg2, arg3)

# Acceptable - under 100 characters (keep on single line)
button.clicked.connect(self.someMethodName)

# Only break when approaching/exceeding 100 characters
result = someFunction(
    arg1, arg2, arg3, arg4, arg5, arg6, arg7
)
```

## Docstring Standards
We follow **Google Style Python docstrings** as documented at:
https://sphinxcontrib-napoleon.readthedocs.io/en/latest/example_google.html

### Critical Rule: Empty Line at End
**ALWAYS include 1 empty line at the end of docstrings** (unless it's a single-line docstring):

```python
# Single-line docstring - NO empty line needed
def simpleFunction(self):
    """Brief description of the function."""
    return "result"

# Multi-line docstring - empty line required at end
def myFunction(arg1: str, arg2: int) -> str:
    """Brief description of the function.

    Args:
        arg1: Description of first argument.
        arg2: Description of second argument.

    Returns:
        Description of return value.

    """
    return "result"
```

### Google Style Sections
Use these standard sections in order (only include sections that are relevant):

1. **Summary line**: One-line summary that fits on one line
2. **Extended description** (optional): More details after a blank line
3. **Args**: Function/method parameters
4. **Returns**: Return value description
5. **Yields**: For generators (instead of Returns)
6. **Raises**: Exceptions that may be raised
7. **Note** or **Notes**: Additional notes
8. **Example** or **Examples**: Usage examples
9. **Attributes**: For classes, document public attributes
10. **Todo**: Future improvements

### Args Section Format
```python
def exampleFunction(param1, param2=None, *args, **kwargs):
    """Function with various parameter types.

    Args:
        param1 (int): The first parameter.
        param2 (str, optional): The second parameter. Defaults to None.
            Second line of description should be indented.
        *args: Variable length argument list.
        **kwargs: Arbitrary keyword arguments.

    Returns:
        bool: True if successful, False otherwise.

    """
```

### Args Section with Type Hints
When using PEP 484 type hints, types are optional in docstring:

```python
def exampleFunction(param1: int, param2: str = None) -> bool:
    """Function with type hints.

    Args:
        param1: The first parameter (type from annotation).
        param2: The second parameter (type from annotation). Defaults to None.

    Returns:
        True if successful, False otherwise.

    """
```

### Returns Section Format
```python
def getResult():
    """Get a result value.

    Returns:
        dict: Dictionary containing:
            - 'success' (bool): Whether operation succeeded
            - 'message' (str): Status message
            - 'data' (list): Result data

    """
```

### Raises Section Format
```python
def validateInput(value):
    """Validate input value.

    Args:
        value: The value to validate.

    Raises:
        ValueError: If value is negative.
        TypeError: If value is not a number.

    """
```

### Class Docstrings
```python
class ExampleClass:
    """Summary line for the class.

    Extended description of the class purpose and usage.

    Attributes:
        attr1 (str): Description of attr1.
        attr2 (int, optional): Description of attr2.

    """

    def __init__(self, param1, param2):
        """Initialize the ExampleClass.

        Note:
            Do not include the `self` parameter in Args section.

        Args:
            param1 (str): Description of param1.
            param2 (int): Description of param2.

        """
        self.attr1 = param1
        self.attr2 = param2
```

### Method Docstrings
```python
class ExampleClass:
    def exampleMethod(self, param1):
        """Brief description of the method.

        Note:
            Do not include the `self` parameter in Args section.

        Args:
            param1: The first parameter.

        Returns:
            True if successful, False otherwise.

        """
        return True
```

### Property Docstrings
```python
@property
def my_property(self):
    """str: Properties should be documented in their getter method.

    The type can be specified at the start of the summary line.

    """
    return self._my_property
```

### Function Overrides
Indicate function overrides in docstrings:

```python
def someMethod(self, param1):
    """Override: Method description.

    This overrides the parent class method to provide custom behavior.

    Args:
        param1: Description of parameter.

    Returns:
        Description of return value.

    """
```

### Examples Section

Use pure Google Style doctest examples within docstrings.

```python
def exampleFunction(n):
    """Generate numbers from 0 to n-1.

    Args:
        n (int): The upper limit of the range to generate.

    Yields:
        int: The next number in the range of 0 to n-1.

    Examples:
    - Basic usage:
        >>> print([i for i in exampleFunction(4)])
        [0, 1, 2, 3]

    """
    for i in range(n):
        yield i
```

**Multiple Examples with Doctest Formatting:**

Use bullet points (`-`) with doctest examples.

```python
class DatabaseInterface(metaclass=ABCSingleton):
    """Thread-safe singleton with abstract methods.

    Examples:
    - Basic usage:
        >>> db1 = MySQLDatabase()
        >>> db1.query_count = 5
        
        >>> db2 = MySQLDatabase()
        >>> print(db2.query_count)  # 5 (same instance)
        5
        >>> print(db1 is db2)
        True
    
    - Force re-initialization:
        >>> db3 = MySQLDatabase(_reinit=True)
        >>> print(db3.query_count)  # 0 (fresh instance)
        0

    """
```

### Note Section
```python
def complexFunction(data):
    """Process complex data.

    Args:
        data: The data to process.

    Returns:
        Processed data.

    Note:
        This function modifies the input data in-place for performance.
        Make a copy if you need to preserve the original.

    """
```

### Exception Standards
**Try to catch specific exceptions rather than using a broad `Exception` catch.**

### DRY Principle (Don't Repeat Yourself)
**Prioritize code reuse and avoid duplication whenever possible:**

```python
# BAD - Duplicated logic in multiple methods
def processInTabAssets(self, data):
    for item in data:
        tree_item = QtWidgets.QTreeWidgetItem(self.tree)
        tree_item.setText(0, item.name)
        tree_item.setExpanded(True)
        # ... 20 lines of setup logic

def processFromFile(self, data):
    for item in data:
        tree_item = QtWidgets.QTreeWidgetItem(self.tree)
        tree_item.setText(0, item.name)
        tree_item.setExpanded(True)
        # ... same 20 lines of setup logic (DUPLICATED!)

# GOOD - Extract common logic into reusable helper method
def _createTreeItem(self, tree_widget, item_data):
    """Helper method to create and configure tree items."""
    tree_item = QtWidgets.QTreeWidgetItem(tree_widget)
    tree_item.setText(0, item_data.name)
    tree_item.setExpanded(True)
    # ... setup logic in one place
    return tree_item

def processInTabAssets(self, data):
    for item in data:
        self._createTreeItem(self.tree, item)

def processFromFile(self, data):
    for item in data:
        self._createTreeItem(self.tree, item)
```

**Code Reuse Strategy:**
2. **Extract common patterns** - If you see repeated code, create a helper method
3. **Update existing code** - When creating new reusable functions, update older sections to use them
4. **Prefer composition over copy-paste** - Call existing methods rather than duplicating logic
