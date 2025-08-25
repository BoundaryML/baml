from typing import Dict, Any, Optional
from .runtime import BamlRuntime
from .serde.type_map import TypeMap


class ScopedClient:
    """Client with a bound type map for a specific scope."""
    
    def __init__(self, runtime: BamlRuntime, type_map: TypeMap):
        """
        Initialize a ScopedClient with a runtime and type map.
        
        Args:
            runtime: The BAML runtime instance
            type_map: The type map to use for encoding/decoding in this scope
        """
        self._runtime = runtime
        self._type_map = type_map
    
    async def call_function(
        self, 
        name: str, 
        args: Dict[str, Any],
        arg_types: Dict[str, str], 
        return_type: str,
        env_vars: Optional[Dict[str, str]] = None
    ) -> Any:
        """
        Call function using bound type map.
        
        Args:
            name: The function name to call
            args: Dictionary of function arguments
            arg_types: Dictionary mapping argument names to their type names
            return_type: The expected return type name
            env_vars: Optional environment variables to pass
            
        Returns:
            The decoded result using the bound type map
        """
        return await self._runtime.call_function_typed(
            name, args, self._type_map, arg_types, return_type, env_vars
        )
    
    @property
    def type_map(self) -> TypeMap:
        """Get the bound type map for this client."""
        return self._type_map
    
    @property
    def runtime(self) -> BamlRuntime:
        """Get the underlying runtime instance."""
        return self._runtime