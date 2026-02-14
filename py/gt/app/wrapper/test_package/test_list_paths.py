"""Test list-based paths and Unix path normalization."""
import sys
import os
from pathlib import Path

# Add the module to path for testing
sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent))

from gt.app.wrapper import WrapperConfig, ApplicationWrapper

# Set up test environment variable
os.environ['CUSTOM_PATH'] = 'C:\\original\\path'
os.environ['PRIORITY_PATH'] = 'C:\\original\\priority'

config = WrapperConfig(
    executable='python',
    args=['-c', '''
import os
print("PYTHONPATH:", os.environ.get("PYTHONPATH"))
print()
print("CUSTOM_PATH:", os.environ.get("CUSTOM_PATH"))
print()
print("PRIORITY_PATH:", os.environ.get("PRIORITY_PATH"))
print()
print("SIMPLE_VAR:", os.environ.get("SIMPLE_VAR"))
'''],
    env_files='gt/app/wrapper/test_list_paths.json',
    capture_output=True,
    stream_output=False
)

wrapper = ApplicationWrapper(config)
result = wrapper.run()

print("Test Results:")
print("=" * 60)
print(result.stdout)
print("=" * 60)
print()
print("Expected behavior:")
print("1. PYTHONPATH: List joined with ; (on Windows)")
print("2. Unix paths (/) converted to Windows paths (\\) on Windows")
print("3. CUSTOM_PATH: original + appended list")
print("4. PRIORITY_PATH: prepended + original")
