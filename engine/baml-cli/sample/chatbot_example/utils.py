from contextlib import contextmanager
from itertools import cycle
import sys
import threading
import time

from termcolor import colored
from baml_client.baml_types import Attendee

def find_attendee_by_email(email: str) -> Attendee:
    return Attendee(email=email, name=email.split('@')[0])


@contextmanager
def loading_animation(text, color='blue'):
    done = False

    def animate():
        for frame in cycle(['', '.', '..', '...']):
            if done:
                break
            sys.stdout.write('\r' + colored(text + frame, color))
            sys.stdout.flush()
            time.sleep(0.5)
        sys.stdout.write('\r\033[K')  # Clear the line

    t = threading.Thread(target=animate)
    t.start()

    try:
        yield
    finally:
        done = True
        t.join()
