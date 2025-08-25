.. _installation_medium:

Installation Medium
-------------------

Proxmox Datacenter Manager can be installed via :ref:`different methods <install_pdm>`. The
recommended method is the usage of an installation medium, to simply boot the interactive installer.

Prepare Installation Medium
~~~~~~~~~~~~~~~~~~~~~~~~~~~

Download the installer ISO image from |DOWNLOADS|.

The Proxmox Datacenter Manager installation medium is a hybrid ISO image.  It works in two ways:

- An ISO image file ready to burn to a DVD.

- A raw sector (IMG) image file ready to copy to a USB flash drive (USB stick).

Using a USB flash drive to install Proxmox Datacenter Manager is the recommended way since it is the
faster and more frequently available option these days.

Prepare a USB Flash Drive as Installation Medium
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

The flash drive needs to have at least 2 GB of storage space.

.. note::

   Do not use *UNetbootin*. It does not work with the Proxmox Datacenter Manager installation image.

.. important::

   Existing data on the USB flash drive will be overwritten. Therefore, make sure that it does not
   contain any still needed data and unmount it afterwards again before proceeding.

Instructions for GNU/Linux
~~~~~~~~~~~~~~~~~~~~~~~~~~

On Unix-like operating systems use the ``dd`` command to copy the ISO image to the USB flash drive.
First find the correct device name of the USB flash drive (see below). Then run the ``dd`` command.
Depending on your environment, you will need to have root privileges to execute ``dd``.

.. code-block:: console

   # dd bs=1M conv=fdatasync if=./proxmox-datacenter-manager_*.iso of=/dev/XYZ

.. note::

   Be sure to replace ``/dev/XYZ`` with the correct device name and adapt
   the input filename (*if*) path.

.. caution::

   Be very careful, and do not overwrite the wrong disk!

Find the Correct USB Device Name
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

There are two ways to find out the name of the USB flash drive. The first one is to compare the last
lines of the ``dmesg`` command output before and after plugging in the flash drive. The second way
is to compare the output of the ``lsblk`` command. Open a terminal and run:

.. code-block:: console

   # lsblk

Then plug in your USB flash drive and run the command again:

.. code-block:: console

   # lsblk

A new device will appear. This is the one you want to use. To be on the extra safe side check if the
reported size matches your USB flash drive.

Instructions for macOS
~~~~~~~~~~~~~~~~~~~~~~

Open the terminal (query *Terminal* in Spotlight).

Convert the ``.iso`` file to ``.dmg`` format using the convert option of ``hdiutil``, for example:

.. code-block:: console

   # hdiutil convert proxmox-datacenter-manager_*.iso -format UDRW -o proxmox-datacenter-manager_*.dmg

.. note::

   macOS tends to automatically add ``.dmg`` to the output file name.

To get the current list of devices run the command:

.. code-block:: console

   # diskutil list

Now insert the USB flash drive and run this command again to determine which device node has been
assigned to it. (e.g., ``/dev/diskX``).

.. code-block:: console

   # diskutil list
   # diskutil unmountDisk /dev/diskX

.. note::

   replace *X* with the disk number from the last command.

.. code-block:: console

   # sudo dd if=proxmox-datacenter-manager_*.dmg bs=1M of=/dev/rdiskX

.. note::

   *rdiskX*, instead of *diskX*, in the last command is intended. It will increase the write speed.

Instructions for Windows
~~~~~~~~~~~~~~~~~~~~~~~~

Using Etcher
^^^^^^^^^^^^

Etcher works out of the box. Download Etcher from https://etcher.io. It will guide you through the
process of selecting the ISO and your USB flash drive.

Using Rufus
^^^^^^^^^^^

Rufus is a more lightweight alternative, but you need to use the **DD mode** to make it work.
Download Rufus from https://rufus.ie/. Either install it or use the portable version. Select the
destination drive and the downloaded Proxmox ISO file.

.. important::

   Once you click *Start*, you have to click *No* on the dialog asking to download a different
   version of Grub. In the next dialog select **DD mode**.

Use the Installation Medium
~~~~~~~~~~~~~~~~~~~~~~~~~~~

Insert the created USB flash drive (or DVD) into your server. Continue by reading the
:ref:`installer <using_the_installer>` chapter, which also describes possible boot issues.
