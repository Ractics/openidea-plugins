<?php
// Explicit "Send Request": runs one route through App\Kernel inside this process, the way
// public/index.php does. No server and no network. With rollback on, nothing is sent unless a
// Doctrine transaction really started on every connection.
$input = json_decode($argv[1] ?? '', true);
// The reply line carries the caller's nonce, so text the app prints can't pass for it.
$nonce = is_array($input) && is_string($input['nonce'] ?? null) && preg_match('/^[0-9a-f]{32}$/', $input['nonce']) ? $input['nonce'] : 'invalid';
$marker = "\n@@symfony-request-{$nonce}@@";
$reply = static function (array $payload) use ($marker): void {
    fwrite(STDOUT, $marker.json_encode($payload, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_INVALID_UTF8_SUBSTITUTE | JSON_PARTIAL_OUTPUT_ON_ERROR)."\n");
};
$notSent = static function (string $reason) use ($reply): void {
    $reply(['notSent' => true, 'error' => $reason]);
    exit(0);
};
// Messages can carry a DSN: everything up to the last @ of a URL's authority goes (a password may
// contain @), and the text is shortened.
$clean = static function (Throwable $e, int $length): string {
    return substr(preg_replace('#//[^\s/]*@#', '//•••@', $e->getMessage()) ?? '', 0, $length);
};
if (!is_array($input)) { $notSent('Nothing was sent: the request was not understood.'); }
error_reporting(E_ALL & ~E_DEPRECATED & ~E_USER_DEPRECATED);
ini_set('display_errors', 'stderr');

$method = strtoupper((string) ($input['method'] ?? ''));
$uri = (string) ($input['uri'] ?? '');
$query = ltrim((string) ($input['query'] ?? ''), '?');
$body = $input['body'] ?? null;
$json = (bool) ($input['json'] ?? true);
$rollback = ($input['rollback'] ?? true) !== false;
$userId = $input['user'] ?? null;
if (!in_array($method, ['GET', 'HEAD', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'], true) || $uri === '' || $uri[0] !== '/' || strlen($uri) > 2048 || strlen($query) > 4096 || preg_match('/[\x00-\x20]/', $uri)) {
    $notSent('Nothing was sent: the method or URI is not valid.');
}
if ($userId !== null && (!is_string($userId) || $userId === '' || strlen($userId) > 1024 || preg_match('/[\x00-\x1f\x7f]/', $userId))) {
    $notSent('Nothing was sent: the user identifier is not valid.');
}

$root = getcwd();
try {
    if (!is_file($root.'/vendor/autoload.php')) {
        throw new RuntimeException('vendor/autoload.php is missing; run composer install');
    }
    require $root.'/vendor/autoload.php';
    if (is_file($root.'/config/bootstrap.php')) {
        // Symfony 4 apps load .env and set APP_ENV/APP_DEBUG here.
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
    $notSent('Nothing was sent: the app did not boot ('.$clean($e, 300).').');
}

// Every Doctrine connection gets its own transaction: a write through any of them is rolled back.
$connections = [];
$rollBackAll = static function () use (&$connections): bool {
    $all = true;
    foreach ($connections as $connection) {
        try {
            $open = $connection->isTransactionActive();
            for ($i = 0; $i < 64 && $connection->isTransactionActive(); $i++) { $connection->rollBack(); }
            $all = $all && $open && !$connection->isTransactionActive();
        } catch (Throwable $e) {
            $all = false;
        }
    }
    return $all;
};
if ($rollback) {
    try {
        $opening = [];
        if ($container->has('doctrine')) {
            $registry = $container->get('doctrine');
            $names = method_exists($registry, 'getConnectionNames') ? array_map('strval', array_keys($registry->getConnectionNames())) : ['default'];
            if (count($names) > 32) {
                throw new RuntimeException('the app has more than 32 connections');
            }
            foreach ($names as $name) {
                try {
                    $opening[$name] = $registry->getConnection($name);
                } catch (Throwable $e) {
                    throw new RuntimeException('connection '.$name.': '.$e->getMessage());
                }
            }
        } elseif ($container->has('doctrine.dbal.default_connection')) {
            $opening['default'] = $container->get('doctrine.dbal.default_connection');
        } else {
            throw new RuntimeException('Doctrine is not installed');
        }
        foreach ($opening as $name => $connection) {
            if (!method_exists($connection, 'beginTransaction') || !method_exists($connection, 'isTransactionActive') || !method_exists($connection, 'rollBack')) {
                throw new RuntimeException('connection '.$name.' has no transactions');
            }
            $connections[$name] = $connection;
            try {
                $connection->beginTransaction();
            } catch (Throwable $e) {
                throw new RuntimeException('connection '.$name.': '.$e->getMessage());
            }
            if (!$connection->isTransactionActive()) {
                throw new RuntimeException('connection '.$name.' reports no open transaction');
            }
        }
    } catch (Throwable $e) {
        $rollBackAll();
        $notSent('Nothing was sent: Roll back database changes is on, but a transaction could not be started on every Doctrine connection ('.$clean($e, 300).'). Turn Roll back database changes off to send the request for real.');
    }
}

// Reports whether every transaction was still open to roll back; false when the app ended one itself.
$finish = static function () use (&$connections, $rollBackAll): ?bool {
    return $connections === [] ? null : $rollBackAll();
};

$identifier = static function ($user): ?string {
    if (is_object($user) && method_exists($user, 'getUserIdentifier')) { return (string) $user->getUserIdentifier(); }
    if (is_object($user) && method_exists($user, 'getUsername')) { return (string) $user->getUsername(); }
    return null;
};

// Best effort: a public user provider when the app has one, else a Doctrine entity that implements
// UserInterface, through its repository's loader or an email/username/identifier field.
$loadUser = static function (string $id) use ($container) {
    if ($container->has('security.user_providers')) {
        $provider = $container->get('security.user_providers');
        try {
            return method_exists($provider, 'loadUserByIdentifier') ? $provider->loadUserByIdentifier($id) : $provider->loadUserByUsername($id);
        } catch (Throwable $e) {
        }
    }
    if (!$container->has('doctrine') || !interface_exists('Symfony\Component\Security\Core\User\UserInterface')) {
        return null;
    }
    foreach ($container->get('doctrine')->getManagers() as $manager) {
        foreach ($manager->getMetadataFactory()->getAllMetadata() as $metadata) {
            $class = $metadata->getName();
            if (!is_subclass_of($class, 'Symfony\Component\Security\Core\User\UserInterface') || !empty($metadata->isMappedSuperclass)) { continue; }
            $repository = $manager->getRepository($class);
            if (method_exists($repository, 'loadUserByIdentifier') && ($user = $repository->loadUserByIdentifier($id))) { return $user; }
            if (method_exists($repository, 'loadUserByUsername') && ($user = $repository->loadUserByUsername($id))) { return $user; }
            foreach (['email', 'username', 'identifier'] as $field) {
                if ($metadata->hasField($field) && ($user = $repository->findOneBy([$field => $id]))) { return $user; }
            }
        }
    }
    return null;
};

try {
    $server = ['HTTP_ACCEPT' => $json ? 'application/json' : 'text/html,application/xhtml+xml', 'REMOTE_ADDR' => '127.0.0.1', 'HTTP_USER_AGENT' => 'Symfony OpenIDEA request'];
    $content = null;
    $parameters = [];
    if (is_array($body) && !in_array($method, ['GET', 'HEAD'], true)) {
        if ($json) {
            $server['CONTENT_TYPE'] = 'application/json';
            $content = json_encode($body);
        } else {
            $server['CONTENT_TYPE'] = 'application/x-www-form-urlencoded';
            $parameters = $body;
        }
    }
    $request = Symfony\Component\HttpFoundation\Request::create($uri.($query !== '' ? '?'.$query : ''), $method, $parameters, [], [], $server, $content);

    $warnings = [];
    $token = null;
    $storage = $container->has('security.token_storage') ? $container->get('security.token_storage') : null;
    if ($userId !== null) {
        $tokenClass = class_exists('Symfony\Component\Security\Core\Authentication\Token\UsernamePasswordToken')
            ? 'Symfony\Component\Security\Core\Authentication\Token\UsernamePasswordToken'
            : 'Symfony\Component\Security\Http\Authenticator\Token\PostAuthenticationToken';
        if ($storage === null || !class_exists($tokenClass)) {
            $finish();
            $notSent('Nothing was sent: Act as user needs symfony/security-bundle.');
        }
        try {
            $user = $loadUser($userId);
        } catch (Throwable $e) {
            $user = null;
        }
        if ($user === null) {
            $finish();
            $notSent('Nothing was sent: no user '.$userId.' was found through a public user provider or a Doctrine user entity.');
        }
        $token = new $tokenClass($user, 'main', $user->getRoles());
        $storage->setToken($token);
    }

    $started = microtime(true);
    $response = $kernel->handle($request);
    $ms = round((microtime(true) - $started) * 1000, 1);

    if ($token !== null) {
        $after = $storage->getToken();
        if ($after === null || $identifier($after->getUser()) !== $identifier($token->getUser())) {
            $warnings[] = 'The firewall replaced the Act as user token, so the request did not run as '.$userId.'.';
        }
    }

    if ($response instanceof Symfony\Component\HttpFoundation\BinaryFileResponse) {
        $text = '(file '.$response->getFile()->getFilename().', '.$response->getFile()->getSize().' bytes)';
    } elseif ($response instanceof Symfony\Component\HttpFoundation\StreamedResponse) {
        ob_start();
        $response->sendContent();
        $text = (string) ob_get_clean();
    } else {
        $text = (string) $response->getContent();
    }
    $size = strlen($text);
    if ($size > 200000) { $text = substr($text, 0, 200000); }

    // Cookies and credential headers are never returned, only the cookie names.
    $masked = ['set-cookie', 'cookie', 'authorization', 'proxy-authorization', 'x-csrf-token', 'x-xsrf-token'];
    $headers = [];
    foreach (array_slice($response->headers->all(), 0, 64, true) as $name => $values) {
        if (in_array(strtolower((string) $name), $masked, true)) { continue; }
        $headers[$name] = substr(implode(', ', $values), 0, 2000);
    }
    $cookies = [];
    foreach (array_slice($response->headers->getCookies(), 0, 32) as $cookie) {
        $cookies[] = $cookie->getName().'=•••';
    }
    if ($cookies !== []) { $headers['set-cookie'] = implode(', ', $cookies); }

    if ($kernel instanceof Symfony\Component\HttpKernel\TerminableInterface) {
        $kernel->terminate($request, $response);
    }
    $rolledBack = $finish();

    $reply([
        'status' => $response->getStatusCode(),
        'reason' => Symfony\Component\HttpFoundation\Response::$statusTexts[$response->getStatusCode()] ?? '',
        'headers' => $headers,
        'body' => $text,
        'size' => $size,
        'ms' => $ms,
        'redirect' => $response->headers->get('Location'),
        'warnings' => $warnings,
        'rollbackRequested' => $rollback,
        'rolledBack' => $rolledBack,
    ]);
} catch (Throwable $e) {
    $rolledBack = $finish();
    $reply(['error' => get_class($e).': '.$clean($e, 500),'rollbackRequested' => $rollback, 'rolledBack' => $rolledBack]);
}
