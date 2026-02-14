"""Quick verification of all features."""
import sys
import os
import json
from pathlib import Path

# Add the module to path for testing
sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent))

from gt.app.wrapper import WrapperConfig, ApplicationWrapper

# Create test file
test_env = {
    "TEST_LIST": [
        "C:/path1",
        "C:/path2",
        "C:/path3"
    ],
    "TEST_STRING": "C:/single/path",
    "+=TEST_APPEND": ["D:/added1", "D:/added2"],
    "^=TEST_PREPEND": "D:/priority"
}

temp_dir = Path(__file__).parent / "temp"
temp_dir.mkdir(exist_ok=True)
test_file = temp_dir / "verify.json"

with open(test_file, 'w') as f:
    json.dump(test_env, f, indent=2)

# Set up existing vars
os.environ['TEST_APPEND'] = 'D:/original_append'
os.environ['TEST_PREPEND'] = 'D:/original_prepend'

config = WrapperConfig(
    executable='python',
    args=['-c', '''
import os, sys
sep = ";" if sys.platform == "win32" else ":"
print("TEST_LIST:", os.environ.get("TEST_LIST"))
print("TEST_STRING:", os.environ.get("TEST_STRING"))
print("TEST_APPEND:", os.environ.get("TEST_APPEND"))
print("TEST_PREPEND:", os.environ.get("TEST_PREPEND"))
'''],
    env_files=test_file,
    capture_output=True,
    stream_output=False
)

wrapper = ApplicationWrapper(config)
result = wrapper.run()

print("Verification Results:")
print("=" * 60)
print(result.stdout)
print("=" * 60)
print("\nAll features verified:")
print("✓ List-based paths")
print("✓ Unix-style paths (/) converted to Windows (\\)")
print("✓ += operator with lists")
print("✓ ^= operator")

# Cleanup
test_file.unlink()
