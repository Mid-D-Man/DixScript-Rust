# Namespace package — allows other midmanstudio.* packages to coexist.
from pkgutil import extend_path
__path__ = extend_path(__path__, __name__)
