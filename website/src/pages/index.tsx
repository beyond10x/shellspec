import type {ReactNode} from 'react';
import ProductLandingPage from '@theme/ProductLandingPage';
import type {ProductLandingData} from '@beyond10x/docs-system/product-data';
import landingJson from '@site/product.json';

const landing = landingJson as unknown as ProductLandingData;

/**
 * The landing page: product.json drawn by the docs-system product template. It is imported here
 * rather than passed as withProductSite's `landing` option, because that option registers the
 * data under its absolute path on the build machine and the path then ships in the bundle.
 */
export default function Home(): ReactNode {
  return <ProductLandingPage landing={landing} />;
}
