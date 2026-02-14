"""Test the += and ^= operators."""
import sys
import os
from pathlib import Path

# Add the module to path for testing
sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent))

from gt.app.wrapper import WrapperConfig, ApplicationWrapper

# Set up test environment variable
os.environ['TEST_PATH'] = 'original_value'

# Test append operator
print("Test 1: Append operator (+=)")
config1 = WrapperConfig(
    executable='python',
    args=['-c', 'import os; print(f"TEST_PATH: {os.environ.get(\'TEST_PATH\')}")'],
    env_files='gt/app/wrapper/example_env_operator_append.json',
    capture_output=True,
    stream_output=False
)

wrapper1 = ApplicationWrapper(config1)
result1 = wrapper1.run()
print(result1.stdout)
print()

# Reset and test prepend operator
os.environ['TEST_PATH'] = 'original_value'
print("Test 2: Prepend operator (^=)")
config2 = WrapperConfig(
    executable='python',
    args=['-c', 'import os; print(f"TEST_PATH: {os.environ.get(\'TEST_PATH\')}")'],
    env_files='gt/app/wrapper/example_env_operator_prepend.json',
    capture_output=True,
    stream_output=False
)

wrapper2 = ApplicationWrapper(config2)
result2 = wrapper2.run()
print(result2.stdout)
