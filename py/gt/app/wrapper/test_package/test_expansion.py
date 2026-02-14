"""Quick test of environment variable expansion."""
import sys
import os
from pathlib import Path

# Add the module to path for testing
sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent))

from gt.app.wrapper import WrapperConfig, ApplicationWrapper

# Set up test environment variable
os.environ['TEST_VAR'] = 'original_value'

config = WrapperConfig(
    executable='python',
    args=['-c', 'import os; print(f"TEST_VAR: {os.environ.get(\'TEST_VAR\')}"); print(f"NEW_VAR: {os.environ.get(\'NEW_VAR\')}")'],
    env_files='gt/app/wrapper/test_append.json',
    capture_output=True,
    stream_output=False
)

wrapper = ApplicationWrapper(config)
result = wrapper.run()

print("Test Results:")
print(result.stdout)
print()
print("Expected TEST_VAR to be: original_value;appended_value")
print("Expected NEW_VAR to be: test_value")
