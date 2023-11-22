from baml_client.baml_types import Attendee

def find_attendee_by_email(email: str) -> Attendee:
    return Attendee(email=email, name=email.split('@')[0])
