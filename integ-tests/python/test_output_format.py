from enum import Enum
from typing import Optional, Union
from baml_py.type_builder import TypeBuilder
from pydantic import BaseModel
import json
import test_output_format2

class Address(BaseModel):
    street: str
    city: str
    postal_code: str
    country: str

class Item(BaseModel):
    name: str
    description: Optional[str]
    price: float
    tags: list[str]

class OrderStatus(Enum):
    CREATED = 'created'
    PENDING_FULFILLMENT = 'pending_fulfillment'
    SHIPPED = 'shipped_pending_delivery'
    DELIVERED = 'delivered'

class Order(BaseModel):
    id: int
    items: list[Item]
    shipping_address: Union[str, Address]
    status: OrderStatus
    dummy_status: test_output_format2.OrderStatus

def test_output_format():
    print(json.dumps(Order.model_json_schema(), indent=2))
    output_format = TypeBuilder.from_pydantic_model(Order).output_format()
    print("""
Here is the pydantic code:
          
class Address(BaseModel):
    street: str
    city: str
    postal_code: str
    country: str

class Item(BaseModel):
    name: str
    description: Optional[str]
    price: float
    tags: list[str]

class OrderStatus(Enum):
    CREATED = 'created'
    PENDING_FULFILLMENT = 'pending_fulfillment'
    SHIPPED = 'shipped_pending_delivery'
    DELIVERED = 'delivered'

class Order(BaseModel):
    id: int
    items: list[Item]
    shipping_address: Union[str, Address]
    status: OrderStatus


          
and here is the output model:
    """
          )
    print(output_format)
    assert output_format == ""
