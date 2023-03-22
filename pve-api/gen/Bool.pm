package Bool;

use base 'Exporter';
use overload
    '!' => sub { bool(!$_[0]->$*) },
    'bool' => sub { !!$_[0]->$* },
    '""' => sub { $_[0]->$* ? 'true' : 'false' },
    fallback => 1;

our @EXPORT = qw(bool true false);

sub bool : prototype($) {
    my ($v) = @_;
    $v = !!$v;
    bless \$v, __PACKAGE__;
}

sub true : prototype() { bool(1) }
sub false : prototype() { bool(0) }

sub TO_API_SCHEMA {
    my ($self) = @_;
    return "$self";
}
