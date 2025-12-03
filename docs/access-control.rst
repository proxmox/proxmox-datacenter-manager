
.. _auth_and_access:

Authentication & Access Control
===============================

.. _users:

User Configuration
------------------

Proxmox Datacenter Manager supports several authentication realms, and you need to choose the realm when
you add a new user. Possible realms are:

:pam: Linux PAM standard authentication. Use this if you want to authenticate as a Linux system
      user. The users needs to already exist on the host system.

:pdm: Proxmox Datacenter Manager realm. This type stores hashed passwords in
      ``/etc/proxmox-datacenter-manager/access/shadow.json``.

:openid: OpenID Connect server. Users can authenticate against an external OpenID Connect server.

:ldap: LDAP server. Users can authenticate against external LDAP servers.

:ad: Active Directory server. Users can authenticate against external Active Directory servers.

The `root@pam` superuser has full administration rights on everything, so it's recommended to add
other users with less privileges.

.. _api_tokens:

API Tokens
----------

Any authenticated user can generate API tokens, which can in turn be used to configure various
clients, instead of directly providing the username and password.

API tokens serve two purposes:

#. Easy revocation in case client gets compromised
#. Limit permissions for each client/token within the users' permission

An API token consists of two parts: an identifier consisting of the user name, the realm and a
tokenname (``user@realm!tokenname``), and a secret value. Both need to be provided to the client in
place of the user ID (``user@realm``) and the user password, respectively.

The API token is passed from the client to the server by setting the ``Authorization`` HTTP header
with method ``PDMAPIToken`` to the value ``TOKENID:TOKENSECRET``.

.. _access_control:

Access Control
--------------

By default, new users and API tokens do not have any permissions. Instead you need to specify what
is allowed and what is not.

Proxmox Datacenter Manager uses a role- and path-based permission management system.  An entry in
the permissions table allows a user, group or token to take on a specific role when accessing an
'object' or 'path'. This means that such an access rule can be represented as a triple of '(path,
user, role)', '(path, group, role)' or '(path, token, role)', with the role containing a set of
allowed actions, and the path representing the target of these actions.

.. _acl_privs:

Privileges
~~~~~~~~~~

Privileges are the building blocks of access roles. They are internally used to enforce the actual
permission checks in the API.

.. todo list all privileges.

.. _acl_roles:

Access Roles
~~~~~~~~~~~~

An access role combines one or more privileges into something that can be assigned to a user or API
token on an object path.

Currently, there are only built-in roles, meaning you cannot create your own, custom role.

The following roles exist:

.. todo list all roles.


.. _acl_object_paths:

Objects and Paths
~~~~~~~~~~~~~~~~~

Access permissions are assigned to objects, such as a datastore, namespace or some system resources.

We use filesystem-like paths to address these objects. These paths form a natural tree, and
permissions of higher levels (shorter paths) can optionally be propagated down within this
hierarchy.

Paths can be templated, meaning they can refer to the actual id of a configuration entry. When an
API call requires permissions on a templated path, the path may contain references to parameters of
the API call. These references are specified in curly brackets.

Some examples are:

.. todo add more examples below!

.. table::
  :align: left

  =========================== =========================================================
  ``/system/network``         Access to configure the host network
  ``/views/``                 Access to views.
  ``/views/{id}``             Access to a specific view.
  ``/access/users``           User administration
  ``/access/openid/{id}``     Administrative access to a specific OpenID Connect realm
  =========================== =========================================================

Inheritance
^^^^^^^^^^^

As mentioned earlier, object paths form a file system like tree, and permissions can be inherited by
objects down that tree through the propagate flag, which is set by default. We use the following
inheritance rules:

* Permissions for API tokens are always limited to those of the user.
* Permissions on deeper, more specific levels replace those inherited from an upper level.


Configuration & Management
~~~~~~~~~~~~~~~~~~~~~~~~~~

Access permission information is stored in ``/etc/proxmox-datacenter-manager/access/acl.cfg``.
The file contains 5 fields, separated using a colon (':') as a delimiter. A typical entry takes the
form:

``acl:1:/datastore:john@pdm:Administrator``

The data represented in each field is as follows:

#. ``acl`` identifier
#. A ``1`` or ``0``, representing whether propagation is enabled or disabled, respectively
#. The object on which the permission is set. This can be a specific object (like a single view) or
   a top level object, which with propagation enabled, represents all children of the object also.
#. The user(s)/token(s) for which the permission is set
#. The role being set

You can manage permissions via **Configuration -> Access Control -> Permissions** in the web
interface.

API Token Permissions
~~~~~~~~~~~~~~~~~~~~~

API token permissions are calculated based on ACLs containing their ID, independently of those of
their corresponding user. The resulting permission set on a given path is then intersected with that
of the corresponding user.

In practice this means:

#. API tokens require their own ACL entries
#. API tokens can never do more than their corresponding user

Two-Factor Authentication
-------------------------

Introduction
~~~~~~~~~~~~

With simple authentication, only a password (single factor) is required to successfully claim an
identity (authenticate), for example, to be able to log in as `root@pam` on a specific instance of
Proxmox Datacenter Manager. In this case, if the password gets leaked or stolen, anybody can use it
to log in - even if they should not be allowed to do so.

With two-factor authentication (TFA), a user is asked for an additional factor to verify their
authenticity. Rather than relying on something only the user knows (a password), this extra factor
requires something only the user has, for example, a piece of hardware (security key) or a secret
saved on the user's smartphone. This prevents a remote user from gaining unauthorized access to an
account, as even if they have the password, they will not have access to the physical object (second
factor).

Available Second Factors
~~~~~~~~~~~~~~~~~~~~~~~~

You can set up multiple second factors, in order to avoid a situation in which losing your
smartphone or security key locks you out of your account permanently.

Proxmox Datacenter Manager supports three different two-factor authentication methods:

* TOTP (`Time-based One-Time Password <https://en.wikipedia.org/wiki/Time-based_One-Time_Password>`_).
  A short code derived from a shared secret and the current time, it changes
  every 30 seconds.

* WebAuthn (`Web Authentication <https://en.wikipedia.org/wiki/WebAuthn>`_).  A general standard for
  authentication. It is implemented by various security devices, like hardware keys or trusted
  platform modules (TPM) from a computer or smart phone.

* Single use Recovery Keys. A list of keys which should either be printed out and locked in a secure
  place or saved digitally in an electronic vault.  Each key can be used only once. These are
  perfect for ensuring that you are not locked out, even if all of your other second factors are
  lost or corrupt.

.. todo expand this section and chapter more (see pbs)

