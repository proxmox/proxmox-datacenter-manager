#!/usr/bin/env perl

use strict;
use warnings;

use Carp;
use IPC::Open2;

use Data::Dumper;
$Data::Dumper::Indent = 1;

# Load api components
use PVE::API2;
use PVE::API2::AccessControl;
use PVE::API2::Nodes;
use PVE::API2::NodeConfig;

# This is used to build the pve-storage-content enum:
use PVE::Storage::Plugin;

use lib './gen';
use Schema2Rust;

# Dump api:
my $__API_ROOT = PVE::API2->api_dump(undef, 1);

# Initialize:
Schema2Rust::init_api($__API_ROOT);

# Make errors useful:
local $SIG{__DIE__} = sub { die "$Schema2Rust::__err_path: $_[0]" };
local $SIG{__WARN__} = sub { warn "$Schema2Rust::__err_path: $_[0]" };

# From JSONSchema.pm, but we can't use perl-re directly, particularly `qr//`...
my $CONFIGID_RE = '^(?i:[a-z][a-z0-9_-]+)$';

# Disable `#[api]` generation for now, it's incomplete/untested.
#$Schema2Rust::API = 0;

Schema2Rust::register_format('CIDR' => { code => 'crate::verifiers::verify_cidr' });
Schema2Rust::register_format('mac-addr' => { regex => '^(?i)[a-f0-9][02468ace](?::[a-f0-9]{2}){5}$' });
## Schema2Rust::register_format('pve-acme-alias' => { code => 'verify_pve_acme_alias' });
## Schema2Rust::register_format('pve-acme-domain' => { code => 'verify_pve_acme_domain' });
Schema2Rust::register_format('pve-bridge-id' => { regex => '^[-_.\w\d]+$' });
Schema2Rust::register_format('pve-configid' => { regex => $CONFIGID_RE });
## Schema2Rust::register_format('pve-groupid' => { code => 'verify_pve_groupid' });
## Schema2Rust::register_format('pve-userid' => { code => 'verify_pve_userid' });
## # copied from JSONSchema's verify_pve_node sub:
Schema2Rust::register_format('pve-node' => { regex => '^(?i:[a-z0-9](?i:[a-z0-9\-]*[a-z0-9])?)$' });
## #Schema2Rust::register_format('pve-node' => { code => 'verify_pve_node' });
## Schema2Rust::register_format('pve-priv' => { code => 'verify_pve_privileges' });
## Schema2Rust::register_format('pve-realm' => { code => 'verify_pve_realm' });
##
Schema2Rust::register_format('disk-size' => { regex => '^(\d+(\.\d+)?)([KMGT])?$' });
Schema2Rust::register_format('dns-name' => { code => 'crate::verifiers::verify_dns_name' });
## Schema2Rust::register_format('email' => { code => 'verify_email' });
Schema2Rust::register_format('pve-phys-bits' => { code => 'crate::verifiers::verify_pve_phys_bits' });
Schema2Rust::register_format('pve-qm-bootdev' => { unchecked => 1 });
Schema2Rust::register_format('pve-qm-bootdisk' => { regex => '^(ide|sata|scsi|virtio|efidisk|tpmstate)\d+$' });
Schema2Rust::register_format('pve-qm-usb-device' => { unchecked => 1 });
Schema2Rust::register_format('pve-startup-order' => { unchecked => 1 });
Schema2Rust::register_format('pve-storage-id' => { regex => '^(?i:[a-z][a-z0-9\-_.]*[a-z0-9])$' });
Schema2Rust::register_format('pve-storage-content' => { type => 'StorageContent' });
Schema2Rust::register_format('pve-tag' => { regex => '^(?i)[a-z0-9_][a-z0-9_\-+.]*$' });
Schema2Rust::register_format('pve-volume-id' => { code => 'crate::verifiers::verify_volume_id' });
Schema2Rust::register_format('pve-volume-id-or-qm-path' => { code => 'crate::verifiers::verify_pve_volume_id_or_qm_path' });
## Schema2Rust::register_format('pve-volume-id-or-absolute-path' => { code => 'verify_pve_volume_id_or_absolute_path' });
Schema2Rust::register_format('urlencoded' => { regex => '^[-%a-zA-Z0-9_.!~*\'()]*$' });
Schema2Rust::register_format('pve-cpuset' => { regex => '^(\s*\d+(-\d+)?\s*)(,\s*\d+(-\d+)?\s*)?$' });
##
Schema2Rust::register_format('pve-lxc-mp-string' => { code => 'crate::verifiers::verify_lxc_mp_string' });
## Schema2Rust::register_format('lxc-ip-with-ll-iface' => { regex => ['^(?i:', \'pdm_api_types::IPRE!()', ')$'] });
Schema2Rust::register_format('lxc-ip-with-ll-iface' => { code => 'crate::verifiers::verify_ip_with_ll_iface' });
Schema2Rust::register_format('pve-ct-timezone' => { regex => '^.*/.*$' });
##
Schema2Rust::register_format('storage-pair' => { code => 'crate::verifiers::verify_storage_pair' });

Schema2Rust::register_enum_variant('PveVmCpuConfReportedModel::486' => 'I486');
Schema2Rust::register_enum_variant('QemuConfigEfidisk0Efitype::2m' => 'Mb2');
Schema2Rust::register_enum_variant('QemuConfigEfidisk0Efitype::4m' => 'Mb4');
Schema2Rust::register_enum_variant('QemuConfigHugepages::2' => 'Mb2');
Schema2Rust::register_enum_variant('QemuConfigHugepages::1024' => 'Mb1024');
Schema2Rust::register_enum_variant('QemuConfigRng0Source::/dev/urandom', => 'DevUrandom');
Schema2Rust::register_enum_variant('QemuConfigRng0Source::/dev/random', => 'DevRandom');
Schema2Rust::register_enum_variant('QemuConfigRng0Source::/dev/hwrng', => 'DevHwrng');
Schema2Rust::register_enum_variant('QemuConfigTpmstate0Version::v1.2' => 'V1_2');
Schema2Rust::register_enum_variant('QemuConfigTpmstate0Version::v2.0' => 'V2_0');

## # FIXME: Invent an enum list type for this one
Schema2Rust::register_format('pve-hotplug-features' => { unchecked => 1 });
## # FIXME: Figure out something sane for these
Schema2Rust::register_format('address' => { code => 'crate::verifiers::verify_address' });
Schema2Rust::register_format('ipv4' => { code => 'crate::verifiers::verify_ipv4' });
Schema2Rust::register_format('ipv6' => { code => 'crate::verifiers::verify_ipv6' });
Schema2Rust::register_format('pve-ipv4-config' => { code => 'crate::verifiers::verify_ipv4_config' });
Schema2Rust::register_format('pve-ipv6-config' => { code => 'crate::verifiers::verify_ipv6_config' });

# This is used as both a task status and guest status.
Schema2Rust::generate_enum('IsRunning', { type => 'string', enum => ['running', 'stopped'] });

# We have a textual description of the default value in there, just pick the cgroupv2 one:
Schema2Rust::register_api_override('QemuConfig', '/properties/cpuunits/default', 1024);
Schema2Rust::register_api_override('LxcConfig', '/properties/cpuunits/default', 1024);
Schema2Rust::register_api_extension('LxcConfig', '/properties/lxc/items', {
    description => Schema2Rust::quote_string('A raw lxc config entry'),
});
Schema2Rust::register_api_extension('LxcConfig', '/properties/lxc/items/items', {
    description => Schema2Rust::quote_string('A config key value pair'),
});
Schema2Rust::register_api_override('StartQemu', '/properties/timeout/default', 30);

# pve-storage-content uses verify_
my $storage_content_types = [sort keys PVE::Storage::Plugin::valid_content_types('dir')->%*];
Schema2Rust::generate_enum('StorageContent', { type => 'string', enum => $storage_content_types });

sub api : prototype($$$;%) {
    my ($method, $api_url, $rust_method_name, %extra) = @_;
    return Schema2Rust::api($method, $api_url, $rust_method_name, %extra);
}

# FIXME: this needs the return schema specified first:
api(GET => '/version', 'version', 'return-name' => 'VersionResponse');

# Deal with 'type' in `/cluster/resources` being different between input and output.
Schema2Rust::generate_enum(
    'ClusterResourceKind',
    {
        type => 'string',
        enum => ['vm', 'storage', 'node', 'sdn'],
    }
);
api(GET => '/cluster/resources', 'cluster_resources', 'return-name' => 'ClusterResource');

api(GET => '/nodes', 'list_nodes', 'return-name' => 'ClusterNodeIndexResponse');
# api(
#     GET => '/nodes/{node}/config',
#     'get_node_config',
#     'param-name' => 'GetNodeConfig',
#     'return-name' => 'NodeConfig',
#     # 'return-type' => { type => 'object', properties => PVE::NodeConfig::get_nodeconfig_schema() },
# );
# api(PUT => '/nodes/{node}/config', 'set_node_config', 'param-name' => 'UpdateNodeConfig');
# 
# # low level task api:
# # ?? api(GET    => '/nodes/{node}/tasks/{upid}', 'get_task');
# # TODO: api(DELETE => '/nodes/{node}/tasks/{upid}', 'stop_task', 'param-name' => 'StopTask');
# api(GET => '/nodes/{node}/tasks/{upid}/status', 'get_task_status', 'return-name' => 'TaskStatus');
# api(GET => '/nodes/{node}/tasks/{upid}/log', 'get_task_log', 'return-name' => 'TaskLogLine', attribs => 1);
# 
api(GET => '/nodes/{node}/qemu', 'list_qemu', 'param-name' => 'FixmeListQemu', 'return-name' => 'VmEntry');
api(GET => '/nodes/{node}/qemu/{vmid}/config', 'qemu_get_config', 'param-name' => 'FixmeQemuGetConfig', 'return-name' => 'QemuConfig');
# api(POST => '/nodes/{node}/qemu/{vmid}/config', 'qemu_update_config_async', 'param-name' => 'UpdateQemuConfig');
api(POST => '/nodes/{node}/qemu/{vmid}/status/start',    'start_qemu_async',    'output-type' => 'PveUpid', 'param-name' => 'StartQemu');
api(POST => '/nodes/{node}/qemu/{vmid}/status/stop',     'stop_qemu_async',     'output-type' => 'PveUpid', 'param-name' => 'StopQemu');
api(POST => '/nodes/{node}/qemu/{vmid}/status/shutdown', 'shutdown_qemu_async', 'output-type' => 'PveUpid', 'param-name' => 'ShutdownQemu');
# Schema2Rust::derive('StartQemu' => 'Default');
# Schema2Rust::derive('StopQemu' => 'Default');
# Schema2Rust::derive('ShutdownQemu' => 'Default');
# 
api(GET => '/nodes/{node}/lxc',                         'list_lxc',            'param-name' => 'FixmeListLxc',      'return-name' => 'LxcEntry');
api(GET => '/nodes/{node}/lxc/{vmid}/config',           'lxc_get_config',      'param-name' => 'FixmeLxcGetConfig', 'return-name' => 'LxcConfig');
# api(POST => '/nodes/{node}/lxc/{vmid}/status/start',    'start_lxc_async',     'output-type' => 'PveUpid', 'param-name' => 'StartLxc');
# api(POST => '/nodes/{node}/lxc/{vmid}/status/stop',     'stop_lxc_async',      'output-type' => 'PveUpid', 'param-name' => 'StopLxc');
# api(POST => '/nodes/{node}/lxc/{vmid}/status/shutdown', 'shutdown_lxc_async',  'output-type' => 'PveUpid', 'param-name' => 'ShutdownLxc');
# Schema2Rust::derive('StartLxc' => 'Default');
# Schema2Rust::derive('StopLxc' => 'Default');
# Schema2Rust::derive('ShutdownLxc' => 'Default');
# 
# api(GET => '/storage', 'list_storages', 'return-name' => 'StorageList');
# api(GET => '/access/domains', 'list_domains', 'return-name' => 'ListRealm');
# api(GET => '/access/groups', 'list_groups', 'return-name' => 'ListGroups');
# api(GET => '/access/groups/{groupid}', 'get_group', 'return-name' => 'Group');
# api(GET => '/access/users', 'list_users', 'return-name' => 'ListUsers');
# api(GET => '/access/users/{userid}', 'get_user', 'return-name' => 'User');
# api(POST => '/access/users/{userid}/token/{tokenid}', 'create_token', 'param-name' => 'CreateToken');
# Schema2Rust::derive('CreateToken' => 'Default');

# NOW DUMP THE CODE:
#
# We generate one file for API types, and one for API method calls.

my $type_out_file = '../api/pve-client/src/generated/types.rs';
my $code_out_file = '../api/pve-client/src/generated/code.rs';

# Redirect code generation through rustfmt:
open(my $type_fh, '>', $type_out_file) or die "failed to open '$type_out_file': $!\n";
my $type_pid = open2(
    '>&'.fileno($type_fh),
    my $type_pipe,
    #'cat',
    'rustfmt', '--edition=2021', '--config', 'wrap_comments=true'
);
open(my $code_fh, '>', $code_out_file) or die "failed to open '$code_out_file': $!\n";
my $code_pid = open2(
    '>&'.fileno($code_fh),
    my $code_pipe,
    #'cat',
    'rustfmt', '--edition=2021', '--config', 'wrap_comments=true'
);
close($type_fh);
close($code_fh);

# Create .rs files:
print {$code_pipe} "/// PVE API client\n";
print {$code_pipe} "/// Note that the following API URLs are not handled currently:\n";
print {$code_pipe} "///\n";
print {$code_pipe} "/// ```text\n";
my $unused = Schema2Rust::get_unused_paths();
for my $path (sort keys $unused->%*) {
    print {$code_pipe} "/// - $path\n";
}
print {$code_pipe} "/// ```\n";

# Schema2Rust::dump();
Schema2Rust::print_types($type_pipe);
Schema2Rust::print_methods($code_pipe);
$type_pipe->flush();
$code_pipe->flush();
close($type_pipe);
close($code_pipe);

# Wait for formatters to finish:
do {} while $type_pid != waitpid($type_pid, 0);
do {} while $code_pid != waitpid($code_pid, 0);
