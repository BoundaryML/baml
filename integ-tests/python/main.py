import pickle
from baml_client.globals import DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME

try:
    runtime = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME
    pickled = pickle.dumps(runtime)
except Exception as e:
    print(f"{e}")
