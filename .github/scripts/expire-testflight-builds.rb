#!/usr/bin/env ruby
# frozen_string_literal: true

# Expires every TestFlight build older than the one just uploaded.
#
# TestFlight keeps offering testers whatever is newest *that they have already
# installed*, and a list of twenty stale builds makes "which one am I on?" a
# question at all — which it should not be when the answer decides whether a bug
# report is about code that still exists. Expiring the predecessors leaves
# exactly one build installable, so a tester who has anything else is told to
# update rather than left quietly on last week's binary.
#
# Ruby with nothing but the standard library, deliberately: macOS runners ship
# it, and the alternative is installing a JWT library on every run to save
# fifteen lines of signing.

require 'base64'
require 'json'
require 'net/http'
require 'openssl'
require 'uri'

API = 'https://api.appstoreconnect.apple.com'

def fail_with(message)
  warn "::error::#{message}"
  exit 1
end

KEY_ID = ENV['KEY_ID'].to_s
ISSUER_ID = ENV['ISSUER_ID'].to_s
API_KEY = ENV['API_KEY'].to_s
BUNDLE_ID = ENV['BUNDLE_ID'].to_s
CURRENT = ENV['CURRENT_BUILD'].to_s

fail_with('KEY_ID, ISSUER_ID, API_KEY, BUNDLE_ID and CURRENT_BUILD are required') if
  [KEY_ID, ISSUER_ID, API_KEY, BUNDLE_ID, CURRENT].any?(&:empty?)

current_number = Integer(CURRENT, exception: false)
fail_with("CURRENT_BUILD is not a number: #{CURRENT}") if current_number.nil?

# --- authentication ---------------------------------------------------------

# App Store Connect wants ES256, whose JWS signature is the raw r‖s pair;
# OpenSSL hands back DER, so the two integers have to be unpacked and
# left-padded to the curve size. Skipping the padding works until an r or s
# happens to have a leading zero byte, which is roughly one call in 256 — the
# kind of bug that looks like an intermittent Apple outage.
def sign_es256(key, input)
  der = key.sign(OpenSSL::Digest::SHA256.new, input)
  r, s = OpenSSL::ASN1.decode(der).value.map { |v| v.value.to_s(2) }
  r.rjust(32, "\x00") + s.rjust(32, "\x00")
end

def token
  key = begin
    OpenSSL::PKey::EC.new(API_KEY)
  rescue OpenSSL::PKey::ECError => e
    fail_with("APP_STORE_CONNECT_API_KEY is not a usable .p8 private key: #{e.message}")
  end

  encode = ->(o) { Base64.urlsafe_encode64(JSON.dump(o), padding: false) }
  now = Time.now.to_i
  header = { alg: 'ES256', kid: KEY_ID, typ: 'JWT' }
  # Apple rejects anything more than 20 minutes out; this run needs seconds.
  claims = { iss: ISSUER_ID, iat: now, exp: now + 600, aud: 'appstoreconnect-v1' }

  input = "#{encode.call(header)}.#{encode.call(claims)}"
  "#{input}.#{Base64.urlsafe_encode64(sign_es256(key, input), padding: false)}"
end

JWT = token

class ApiError < StandardError; end

def request(method, path, body = nil)
  uri = URI(path.start_with?('http') ? path : "#{API}#{path}")
  klass = { get: Net::HTTP::Get, patch: Net::HTTP::Patch }.fetch(method)
  req = klass.new(uri)
  req['Authorization'] = "Bearer #{JWT}"
  req['Content-Type'] = 'application/json'
  req.body = JSON.dump(body) if body

  response = Net::HTTP.start(uri.hostname, uri.port, use_ssl: true) { |http| http.request(req) }
  unless response.is_a?(Net::HTTPSuccess)
    raise ApiError, "#{method.to_s.upcase} #{uri.path} returned #{response.code}: #{response.body}"
  end

  response.body.to_s.empty? ? {} : JSON.parse(response.body)
end

# --- find the app -----------------------------------------------------------

begin
  apps = request(:get, "/v1/apps?filter[bundleId]=#{URI.encode_www_form_component(BUNDLE_ID)}")
  app = apps['data']&.first
  fail_with("no app in App Store Connect with bundle id #{BUNDLE_ID}") if app.nil?
  app_id = app['id']

  # --- collect its builds ---------------------------------------------------

  builds = []
  page = "/v1/builds?filter[app]=#{app_id}&limit=200&fields[builds]=version,expired"
  while page
    body = request(:get, page)
    builds.concat(body['data'] || [])
    page = body.dig('links', 'next')
  end
rescue ApiError => e
  fail_with(e.message)
end

puts "#{builds.length} build(s) on record for #{BUNDLE_ID}."

# Only builds numbered below the current one, and only those still live.
#
# Comparing numerically rather than by position, so a run that uploads out of
# order cannot expire something newer than itself. A version that will not parse
# as a number is left alone: it predates this scheme, and guessing at an
# ordering is how the current build gets expired by accident.
stale = builds.select do |b|
  number = Integer(b.dig('attributes', 'version').to_s, exception: false)
  !b.dig('attributes', 'expired') && !number.nil? && number < current_number
end

unparsed = builds.count { |b| Integer(b.dig('attributes', 'version').to_s, exception: false).nil? }
puts "::notice::#{unparsed} build(s) have a non-numeric version and were left alone." if unparsed.positive?

if stale.empty?
  puts "Nothing to expire: no live build precedes #{current_number}."
  exit 0
end

# --- expire them ------------------------------------------------------------

failed = 0
stale.sort_by { |b| b.dig('attributes', 'version').to_i }.each do |b|
  version = b.dig('attributes', 'version')
  begin
    request(:patch, "/v1/builds/#{b['id']}", {
      data: { type: 'builds', id: b['id'], attributes: { expired: true } }
    })
    puts "Expired build #{version}."
  rescue ApiError => e
    # One build refusing should not hide the others: a single build stuck in
    # processing is a normal thing to meet, and the remaining twenty still want
    # expiring.
    failed += 1
    warn "::warning::could not expire build #{version}: #{e.message}"
  end
end

puts "Expired #{stale.length - failed} of #{stale.length} build(s) preceding #{current_number}."
exit(failed.zero? ? 0 : 1)
