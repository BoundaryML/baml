import pickle
from baml_client import b

try:
    # Step 1: Serialize the client (this usually works)
    print("Pickling baml client...")
    pickled_data = pickle.dump(b, open("baml_client.pkl", "wb"))
    print("✅ Pickling succeeded!")
    
    # Step 2: Deserialize the client (this is where the error occurs)
    print("Unpickling baml client...")
    b2 = pickle.load(open("baml_client.pkl", "rb"))  # <-- This line causes the error
    print("✅ Unpickling succeeded!")
    
    # Step 3: Test that the unpickled client works
    result = b2.ExtractResume("test")
    print("✅ baml_client can be pickled and unpickled!")
    
except Exception as e:
    print(f"❌ baml_client pickle error: {e}")
    import traceback
    traceback.print_exc()
