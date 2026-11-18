<?php
// Database tab and Add Database to IDE: boots App\Kernel like public/index.php and reads the
// Doctrine connections. Read only: schema introspection, the executed migration versions and
// bounded SELECTs. Connection settings leave without password, URL or driver options.
$marker = "\n@@symfony-database@@";
$reply = static function (array $payload) use ($marker): void {
    fwrite(STDOUT, $marker.json_encode($payload, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_INVALID_UTF8_SUBSTITUTE | JSON_PARTIAL_OUTPUT_ON_ERROR)."\n");
    exit(0);
};
// Driver messages can carry a DSN: credentials in URLs are removed and the text is shortened.
$clean = static function (Throwable $e): string {
    $message = preg_replace('#//[^/\s@]*@#', '//•••@', $e->getMessage()) ?? '';
    return get_class($e).': '.substr($message, 0, 300);
};
$input = json_decode($argv[1] ?? '', true);
$mode = is_array($input) ? (string) ($input['mode'] ?? '') : '';
$options = is_array($input['options'] ?? null) ? $input['options'] : [];
if (!in_array($mode, ['compare', 'rows', 'sources'], true)) { $reply(['error' => 'The database request was not understood.']); }
error_reporting(E_ALL & ~E_DEPRECATED & ~E_USER_DEPRECATED);
ini_set('display_errors', 'stderr');

$root = getcwd();
try {
    if (!is_file($root.'/vendor/autoload.php')) {
        throw new RuntimeException('vendor/autoload.php is missing; run composer install');
    }
    require $root.'/vendor/autoload.php';
    if (is_file($root.'/config/bootstrap.php')) {
        require $root.'/config/bootstrap.php';
    } elseif (class_exists('Symfony\Component\Dotenv\Dotenv') && is_file($root.'/.env')) {
        $dotenv = new Symfony\Component\Dotenv\Dotenv();
        method_exists($dotenv, 'bootEnv') ? $dotenv->bootEnv($root.'/.env') : $dotenv->loadEnv($root.'/.env');
    }
    $env = (string) ($_SERVER['APP_ENV'] ?? $_ENV['APP_ENV'] ?? 'dev');
    $debug = (bool) ($_SERVER['APP_DEBUG'] ?? $_ENV['APP_DEBUG'] ?? ($env !== 'prod'));
    if (!class_exists('App\Kernel')) {
        throw new RuntimeException('App\Kernel was not found');
    }
    $kernel = new App\Kernel($env, $debug);
    $kernel->boot();
    $container = $kernel->getContainer();
} catch (Throwable $e) {
    $reply(['error' => 'The app did not boot ('.$clean($e).').']);
}
if (!$container->has('doctrine')) { $reply(['error' => 'Doctrine is not installed in this app (no doctrine service).']); }
$registry = $container->get('doctrine');

if ($mode === 'sources') {
    $connections = [];
    $default = method_exists($registry, 'getDefaultConnectionName') ? $registry->getDefaultConnectionName() : 'default';
    foreach (array_slice(array_keys($registry->getConnectionNames()), 0, 32) as $name) {
        try {
            $params = $registry->getConnection($name)->getParams();
            $row = ['name' => (string) $name, 'default' => $name === $default];
            foreach (['driver', 'host', 'port', 'dbname', 'user', 'path'] as $key) {
                if (isset($params[$key]) && is_scalar($params[$key])) { $row[$key] = (string) $params[$key]; }
            }
            $connections[] = $row;
        } catch (Throwable $e) {
            $connections[] = ['name' => (string) $name, 'default' => $name === $default, 'error' => $clean($e)];
        }
    }
    $reply(['connections' => $connections]);
}

try {
    $connection = $registry->getConnection();
    $schema = method_exists($connection, 'createSchemaManager') ? $connection->createSchemaManager() : $connection->getSchemaManager();
    $listed = [];
    foreach ($schema->listTableNames() as $name) {
        if (is_string($name) && strlen($name) <= 512 && !preg_match('/[\x00-\x1f]/', $name)) { $listed[$name] = true; }
    }
    $platform = $connection->getDatabasePlatform();
    if ($mode === 'compare') {
        $truncated = count($listed) > 256;
        $tables = [];
        foreach (array_slice(array_keys($listed), 0, 256) as $name) {
            $columns = array_values(array_map(static fn ($column) => $column->getName(), array_slice($schema->listTableColumns($name), 0, 512)));
            $indexes = [];
            foreach (array_slice($schema->listTableIndexes($name), 0, 64) as $index) {
                $indexes[] = ['columns' => array_values($index->getColumns()), 'primary' => $index->isPrimary()];
            }
            $tables[] = ['name' => $name, 'columns' => $columns, 'indexes' => $indexes];
        }
        $executed = null;
        if ($container->has('doctrine.migrations.dependency_factory')) {
            try {
                $executed = [];
                foreach ($container->get('doctrine.migrations.dependency_factory')->getMetadataStorage()->getExecutedMigrations()->getItems() as $item) {
                    $executed[] = (string) $item->getVersion();
                }
            } catch (Throwable $e) {
                $executed = null;
            }
        }
        if ($executed === null) {
            foreach (['doctrine_migration_versions', 'migration_versions'] as $versions) {
                if (isset($listed[$versions])) {
                    $executed = array_map('strval', $connection->fetchFirstColumn('SELECT version FROM '.$platform->quoteIdentifier($versions)));
                    break;
                }
            }
        }
        $driver = (string) ($connection->getParams()['driver'] ?? (new ReflectionClass($platform))->getShortName());
        $reply(['connection' => method_exists($registry, 'getDefaultConnectionName') ? $registry->getDefaultConnectionName() : 'default', 'driver' => $driver, 'tables' => $tables, 'executed' => $executed === null ? null : array_slice($executed, 0, 5000), 'truncated' => $truncated]);
    }

    $table = $options['table'] ?? null;
    if (!is_string($table) || !isset($listed[$table])) { $reply(['error' => 'The table does not exist in the default connection.']); }
    $size = max(10, min(100, (int) ($options['size'] ?? 25)));
    $page = max(0, min(100000, (int) ($options['page'] ?? 0)));
    $columns = array_values(array_map(static fn ($column) => $column->getName(), array_slice($schema->listTableColumns($table), 0, 128)));
    $secret = '/^(pass|passwd|passwords?|passphrase|passcode|pwd|secrets?|tokens?|apikey|authorization|cookies?|credentials?|private|signature|otp|cvv|cvc|card|cardholder|creditcard|hash|salt|iban)$/';
    $personal = '/^(emails?|documents?|phones?|mobile|birth|birthday|birthdate|address(es)?|zip|zipcode|ssn|passport|ip|dob)$/';
    // Whole key words only: "token_count" is masked by "token", "tokenizer" and "category" are not.
    $masked = static function (string $column) use ($secret, $personal): bool {
        $spaced = preg_replace(['/([a-z0-9])([A-Z])/', '/([a-zA-Z])([0-9])/'], '$1 $2', $column);
        $words = preg_split('/[^a-z0-9]+/', strtolower($spaced ?? $column), -1, PREG_SPLIT_NO_EMPTY) ?: [];
        if ($words === []) { return false; }
        if (end($words) === 'key') { return true; }
        $candidates = $words;
        for ($i = 1; $i < count($words); $i++) { $candidates[] = $words[$i - 1].$words[$i]; }
        foreach ($candidates as $word) { if (preg_match($secret, $word) || preg_match($personal, $word)) { return true; } }
        return false;
    };
    $quote = static fn (string $name): string => method_exists($platform, 'quoteSingleIdentifier') ? $platform->quoteSingleIdentifier($name) : $platform->quoteIdentifier($name);
    $from = $quote($table);
    $total = (int) $connection->fetchOne('SELECT COUNT(*) FROM '.$from);
    $order = '';
    foreach ($schema->listTableIndexes($table) as $index) {
        if ($index->isPrimary()) {
            $order = ' ORDER BY '.implode(', ', array_map($quote, array_values(array_intersect($index->getColumns(), $columns))));
            break;
        }
    }
    $select = 'SELECT '.implode(', ', array_map($quote, $columns)).' FROM '.$from.($order === ' ORDER BY ' ? '' : $order);
    $rows = [];
    foreach ($connection->fetchAllNumeric($platform->modifyLimitQuery($select, $size, $page * $size)) as $row) {
        $cells = [];
        foreach ($columns as $at => $column) {
            $value = $row[$at] ?? null;
            if ($value !== null && $masked($column)) { $cells[] = '•••'; }
            elseif (is_string($value) && !mb_check_encoding($value, 'UTF-8')) { $cells[] = '(binary, '.strlen($value).' bytes)'; }
            elseif (is_string($value) && mb_strlen($value) > 200) { $cells[] = mb_substr($value, 0, 200).'…'; }
            elseif ($value === null || is_scalar($value)) { $cells[] = $value; }
            else { $cells[] = '(unsupported value)'; }
        }
        $rows[] = $cells;
    }
    $reply(['table' => $table, 'columns' => $columns, 'rows' => $rows, 'total' => $total, 'page' => $page, 'size' => $size, 'masked' => array_values(array_filter($columns, $masked))]);
} catch (Throwable $e) {
    $reply(['error' => 'Reading the database failed ('.$clean($e).').']);
}
